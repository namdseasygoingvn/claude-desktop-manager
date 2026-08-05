use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::persist;
use super::profile::{non_empty, random_id};
use super::registry;
use super::types::{CdmError, Result};
use crate::platform;

pub const GROUPS_FILE: &str = "groups.json";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Group {
    pub id: String,
    pub name: String,
    pub profile_ids: Vec<String>,
    pub icon: Option<GroupIcon>,
}

/// The whole groups.json: user folders plus the display order of every profile.
/// Membership (a profile's group) decides which section shows it; `order` decides where.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GroupList {
    pub groups: Vec<Group>,
    pub order: Vec<String>,
}

/// A group's chosen icon: an emoji or a lucide icon name. Persisted as a single-key object
/// (`{"emoji":"🏢"}` / `{"symbol":"folder"}`) so groups.json stays readable and hand-editable.
#[derive(Clone, Debug, PartialEq)]
pub enum GroupIcon {
    Emoji(String),
    Symbol(String),
}

impl Serialize for GroupIcon {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(Some(1))?;
        match self {
            GroupIcon::Emoji(emoji) => map.serialize_entry("emoji", emoji)?,
            GroupIcon::Symbol(symbol) => map.serialize_entry("symbol", symbol)?,
        }
        map.end()
    }
}

impl<'de> Deserialize<'de> for GroupIcon {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Wire {
            emoji: Option<String>,
            symbol: Option<String>,
        }
        match Wire::deserialize(deserializer)? {
            Wire { emoji: Some(emoji), .. } => Ok(GroupIcon::Emoji(emoji)),
            Wire { symbol: Some(symbol), .. } => Ok(GroupIcon::Symbol(symbol)),
            _ => Err(serde::de::Error::custom("GroupIcon has no known key")),
        }
    }
}

pub fn path() -> Result<PathBuf> {
    Ok(platform::current().manager_data_dir()?.join(GROUPS_FILE))
}

pub fn load() -> Result<GroupList> {
    let path = path()?;
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(GroupList::default()),
        Err(err) => {
            return Err(CdmError::Io(format!(
                "cannot read {}: {err}",
                path.display()
            )))
        }
    };

    match serde_json::from_slice::<GroupList>(&bytes) {
        Ok(list) => Ok(list),
        Err(_) => match serde_json::from_slice::<Vec<Group>>(&bytes) {
            // Legacy files were a bare array; migrate to the object shape in place.
            Ok(groups) => {
                let list = migrate(groups, &registry::load()?.profiles);
                save(&list)?;
                Ok(list)
            }
            Err(err) => {
                persist::quarantine(&path, &err.to_string())?;
                Ok(GroupList::default())
            }
        },
    }
}

pub fn save(list: &GroupList) -> Result<()> {
    let dir = platform::current().manager_data_dir()?;
    persist::write_json(&dir, GROUPS_FILE, &list, "groups")
}

/// Load, reconcile memberships and order against the registry, and persist any change so the
/// next load is stable. Stale ids can never surface on any read path, and a profile never
/// arranged by hand (just created or adopted) appears at the end of the order.
pub fn list() -> Result<GroupList> {
    let reg = registry::load()?;
    let existing: HashSet<&str> = reg.profiles.iter().map(|p| p.id.as_str()).collect();
    let mut list = load()?;
    let mut changed = prune(&mut list.groups, &existing);

    let before = list.order.len();
    list.order.retain(|id| existing.contains(id.as_str()));
    changed |= list.order.len() != before;

    let mut known: HashSet<String> = list.order.iter().cloned().collect();
    for profile in &reg.profiles {
        if known.insert(profile.id.clone()) {
            list.order.push(profile.id.clone());
            changed = true;
        }
    }

    if changed {
        save(&list)?;
    }
    Ok(list)
}

/// Pure: legacy array -> object. Keep each group's member order, then append profiles that
/// belong to no group (in registry order).
fn migrate(groups: Vec<Group>, profiles: &[super::types::Profile]) -> GroupList {
    let grouped: HashSet<&str> = groups
        .iter()
        .flat_map(|group| group.profile_ids.iter().map(String::as_str))
        .collect();
    let mut order: Vec<String> =
        groups.iter().flat_map(|group| group.profile_ids.iter().cloned()).collect();
    for profile in profiles {
        if !grouped.contains(profile.id.as_str()) {
            order.push(profile.id.clone());
        }
    }
    GroupList { groups, order }
}

pub fn create(name: &str) -> Result<Group> {
    let name = non_empty(name)?;
    let mut list = list()?;
    let group = Group {
        id: mint_gid(&list.groups),
        name: name.to_string(),
        profile_ids: Vec::new(),
        icon: None,
    };
    list.groups.push(group.clone());
    save(&list)?;
    Ok(group)
}

pub fn rename(id: &str, new_name: &str) -> Result<Group> {
    let name = non_empty(new_name)?;
    let mut list = list()?;
    let index = index_of(&list.groups, id)?;
    list.groups[index].name = name.to_string();
    save(&list)?;
    Ok(list.groups[index].clone())
}

pub fn set_icon(id: &str, icon: Option<GroupIcon>) -> Result<Group> {
    let mut list = list()?;
    let index = index_of(&list.groups, id)?;
    list.groups[index].icon = icon;
    save(&list)?;
    Ok(list.groups[index].clone())
}

pub fn delete(id: &str) -> Result<()> {
    let mut list = list()?;
    let index = index_of(&list.groups, id)?;
    list.groups.remove(index);
    save(&list)
}

/// Move a profile into one group — removing it from every other — and place it in the display
/// order before `before`, or last when `before` is `None`. `group_id` `None` ungroups it.
pub fn reposition(profile_id: &str, group_id: Option<&str>, before: Option<&str>) -> Result<()> {
    let reg = registry::load()?;
    if !reg.profiles.iter().any(|p| p.id == profile_id) {
        return Err(CdmError::ProfileNotFound(profile_id.to_string()));
    }
    let mut list = list()?;
    move_profile(&mut list.groups, profile_id, group_id)?;
    reorder(&mut list.order, profile_id, before);
    save(&list)
}

/// Move a profile into one group, removing it from every other, appending it last.
/// `None` ungroups it.
pub fn assign(profile_id: &str, group_id: Option<&str>) -> Result<()> {
    reposition(profile_id, group_id, None)
}

/// Pure: drop `profile_id` from `order`, then insert it before `before` or append it last.
fn reorder(order: &mut Vec<String>, profile_id: &str, before: Option<&str>) {
    order.retain(|id| id != profile_id);
    match before.and_then(|id| order.iter().position(|current| current == id)) {
        Some(index) => order.insert(index, profile_id.to_string()),
        None => order.push(profile_id.to_string()),
    }
}

/// Pure: remove `profile_id` from every group, then add it to `group_id` when given.
fn move_profile(groups: &mut [Group], profile_id: &str, group_id: Option<&str>) -> Result<()> {
    if let Some(group_id) = group_id {
        let index = index_of(groups, group_id)?;
        if !groups[index].profile_ids.iter().any(|id| id == profile_id) {
            groups[index].profile_ids.push(profile_id.to_string());
        }
    }
    for group in groups.iter_mut() {
        if Some(group.id.as_str()) != group_id {
            group.profile_ids.retain(|id| id != profile_id);
        }
    }
    Ok(())
}

/// Pure: drop memberships in profiles that no longer exist; true when anything was dropped.
fn prune(groups: &mut [Group], existing: &HashSet<&str>) -> bool {
    let mut changed = false;
    for group in groups.iter_mut() {
        let before = group.profile_ids.len();
        group.profile_ids.retain(|id| existing.contains(id.as_str()));
        changed |= group.profile_ids.len() != before;
    }
    changed
}

fn mint_gid(groups: &[Group]) -> String {
    loop {
        let id = format!("g_{}", random_id());
        if !groups.iter().any(|g| g.id == id) {
            return id;
        }
    }
}

fn index_of(groups: &[Group], id: &str) -> Result<usize> {
    groups
        .iter()
        .position(|group| group.id == id)
        .ok_or_else(|| CdmError::GroupNotFound(id.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::types::Profile;

    fn group(id: &str, members: &[&str]) -> Group {
        Group {
            id: id.to_string(),
            name: id.to_string(),
            profile_ids: members.iter().map(|m| m.to_string()).collect(),
            icon: None,
        }
    }

    fn profile(id: &str) -> Profile {
        Profile {
            id: id.to_string(),
            name: id.to_string(),
            dir: id.to_string(),
            created_at: String::new(),
            last_used_at: None,
        }
    }

    #[test]
    fn an_icon_round_trips_as_a_single_key_object() {
        let emoji = serde_json::json!({"emoji": "🏢"});
        assert_eq!(
            serde_json::from_value::<GroupIcon>(emoji.clone()).unwrap(),
            GroupIcon::Emoji("🏢".to_string())
        );
        assert_eq!(serde_json::to_value(GroupIcon::Emoji("🏢".to_string())).unwrap(), emoji);

        let symbol = serde_json::json!({"symbol": "folder"});
        assert_eq!(
            serde_json::from_value::<GroupIcon>(symbol.clone()).unwrap(),
            GroupIcon::Symbol("folder".to_string())
        );
        assert_eq!(
            serde_json::to_value(GroupIcon::Symbol("folder".to_string())).unwrap(),
            symbol
        );
    }

    #[test]
    fn an_icon_without_a_known_key_is_rejected() {
        assert!(serde_json::from_value::<GroupIcon>(serde_json::json!({})).is_err());
    }

    #[test]
    fn prune_drops_stale_memberships() {
        let mut groups = [group("g1", &["p1", "p2", "gone"])];
        let existing: HashSet<&str> = ["p1", "p2"].into_iter().collect();
        assert!(prune(&mut groups, &existing));
        assert_eq!(groups[0].profile_ids, ["p1", "p2"]);

        assert!(!prune(&mut groups, &existing));
        assert_eq!(groups[0].profile_ids, ["p1", "p2"]);
    }

    #[test]
    fn move_profile_reassigns_and_ungroups() {
        let mut groups = [group("g1", &["p1"]), group("g2", &["p2"])];

        move_profile(&mut groups, "p1", Some("g2")).unwrap();
        assert_eq!(groups[0].profile_ids, Vec::<String>::new());
        assert_eq!(groups[1].profile_ids, ["p2", "p1"]);

        move_profile(&mut groups, "p1", None).unwrap();
        assert_eq!(groups[0].profile_ids, Vec::<String>::new());
        assert_eq!(groups[1].profile_ids, ["p2"]);
    }

    #[test]
    fn move_profile_to_a_missing_group_errors() {
        let mut groups = [group("g1", &[])];
        assert!(matches!(
            move_profile(&mut groups, "p1", Some("missing")),
            Err(CdmError::GroupNotFound(_))
        ));
    }

    #[test]
    fn reorder_inserts_before_an_anchor_or_appends_last() {
        let mut order: Vec<String> = ["a", "b", "c"].iter().map(|s| s.to_string()).collect();

        reorder(&mut order, "d", None);
        assert_eq!(order, ["a", "b", "c", "d"]);

        reorder(&mut order, "a", Some("c"));
        assert_eq!(order, ["b", "a", "c", "d"]);

        // An anchor that is not in the order degrades to append.
        reorder(&mut order, "a", Some("nope"));
        assert_eq!(order, ["b", "c", "d", "a"]);
    }

    #[test]
    fn migrate_orders_members_then_ungrouped_profiles() {
        let groups = vec![group("g1", &["p2", "p1"]), group("g2", &["p3"])];
        let profiles = [profile("p1"), profile("p2"), profile("p3"), profile("p4")];

        let list = migrate(groups, &profiles);
        assert_eq!(list.groups.len(), 2);
        assert_eq!(list.order, ["p2", "p1", "p3", "p4"]);
    }
}

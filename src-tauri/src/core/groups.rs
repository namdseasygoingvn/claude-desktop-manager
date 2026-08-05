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

pub fn load() -> Result<Vec<Group>> {
    let path = path()?;
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => {
            return Err(CdmError::Io(format!(
                "cannot read {}: {err}",
                path.display()
            )))
        }
    };

    match serde_json::from_slice::<Vec<Group>>(&bytes) {
        Ok(groups) => Ok(groups),
        Err(err) => {
            persist::quarantine(&path, &err.to_string())?;
            Ok(Vec::new())
        }
    }
}

pub fn save(groups: &[Group]) -> Result<()> {
    let dir = platform::current().manager_data_dir()?;
    persist::write_json(&dir, GROUPS_FILE, &groups, "groups")
}

/// Load, drop memberships in profiles that no longer exist, and persist the prune so the next
/// load is stable. Stale ids can never surface on any read path.
pub fn list() -> Result<Vec<Group>> {
    let reg = registry::load()?;
    let existing: HashSet<&str> = reg.profiles.iter().map(|p| p.id.as_str()).collect();
    let mut groups = load()?;
    if prune(&mut groups, &existing) {
        save(&groups)?;
    }
    Ok(groups)
}

pub fn create(name: &str) -> Result<Group> {
    let name = non_empty(name)?;
    let mut groups = list()?;
    let group = Group {
        id: mint_gid(&groups),
        name: name.to_string(),
        profile_ids: Vec::new(),
        icon: None,
    };
    groups.push(group.clone());
    save(&groups)?;
    Ok(group)
}

pub fn rename(id: &str, new_name: &str) -> Result<Group> {
    let name = non_empty(new_name)?;
    let mut groups = list()?;
    let index = index_of(&groups, id)?;
    groups[index].name = name.to_string();
    save(&groups)?;
    Ok(groups[index].clone())
}

pub fn set_icon(id: &str, icon: Option<GroupIcon>) -> Result<Group> {
    let mut groups = list()?;
    let index = index_of(&groups, id)?;
    groups[index].icon = icon;
    save(&groups)?;
    Ok(groups[index].clone())
}

pub fn delete(id: &str) -> Result<()> {
    let mut groups = list()?;
    let index = index_of(&groups, id)?;
    groups.remove(index);
    save(&groups)
}

/// Move a profile into one group, removing it from every other. `None` ungroups it.
pub fn assign(profile_id: &str, group_id: Option<&str>) -> Result<()> {
    let reg = registry::load()?;
    if !reg.profiles.iter().any(|p| p.id == profile_id) {
        return Err(CdmError::ProfileNotFound(profile_id.to_string()));
    }
    let mut groups = list()?;
    move_profile(&mut groups, profile_id, group_id)?;
    save(&groups)
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

    fn group(id: &str, members: &[&str]) -> Group {
        Group {
            id: id.to_string(),
            name: id.to_string(),
            profile_ids: members.iter().map(|m| m.to_string()).collect(),
            icon: None,
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
}

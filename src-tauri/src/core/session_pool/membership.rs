use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::super::persist;
use super::super::types::Result;
use crate::platform;

pub const SESSION_SYNC_FILE: &str = "session-sync.json";
pub const MEMBERSHIP_VERSION: u32 = 1;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Membership {
    pub version: u32,
    pub profile_ids: Vec<String>,
}

impl Default for Membership {
    fn default() -> Self {
        Self {
            version: MEMBERSHIP_VERSION,
            profile_ids: Vec::new(),
        }
    }
}

pub fn path() -> Result<PathBuf> {
    Ok(platform::current().manager_data_dir()?.join(SESSION_SYNC_FILE))
}

/// Cannot fail: missing or unparseable session-sync.json is Membership::default().
pub fn load() -> Membership {
    let Ok(path) = path() else {
        return Membership::default();
    };
    let Ok(bytes) = fs::read(path) else {
        return Membership::default();
    };
    serde_json::from_slice(&bytes).unwrap_or_default()
}

pub fn save(membership: &Membership) -> Result<()> {
    let dir = platform::current().manager_data_dir()?;
    persist::write_json(&dir, SESSION_SYNC_FILE, membership, "session-sync")
}

/// O(n) scan of the loaded list; n is the profile count (single digits in practice).
pub fn is_member(profile_id: &str) -> bool {
    load().profile_ids.iter().any(|id| id == profile_id)
}

/// Idempotent: no write when `profile_id` is already present.
pub fn add(profile_id: &str) -> Result<()> {
    let mut membership = load();
    if !insert(&mut membership.profile_ids, profile_id) {
        return Ok(());
    }
    save(&membership)
}

/// Idempotent: no write when `profile_id` is already absent.
pub fn remove(profile_id: &str) -> Result<()> {
    let mut membership = load();
    if !remove_id(&mut membership.profile_ids, profile_id) {
        return Ok(());
    }
    save(&membership)
}

fn insert(profile_ids: &mut Vec<String>, profile_id: &str) -> bool {
    if profile_ids.iter().any(|id| id == profile_id) {
        return false;
    }
    profile_ids.push(profile_id.to_string());
    true
}

fn remove_id(profile_ids: &mut Vec<String>, profile_id: &str) -> bool {
    let before = profile_ids.len();
    profile_ids.retain(|id| id != profile_id);
    profile_ids.len() != before
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_install_has_no_members() {
        assert_eq!(Membership::default().version, MEMBERSHIP_VERSION);
        assert!(Membership::default().profile_ids.is_empty());
    }

    #[test]
    fn a_file_written_by_an_older_build_falls_back_to_the_defaults() {
        let parsed: Membership = serde_json::from_str(r#"{"somethingElse":1}"#).unwrap();
        assert_eq!(parsed, Membership::default());
    }

    #[test]
    fn the_stored_key_is_camel_case() {
        let json = serde_json::to_string(&Membership::default()).unwrap();
        assert_eq!(json, r#"{"version":1,"profileIds":[]}"#);
    }

    #[test]
    fn insert_is_idempotent_on_a_second_call() {
        let mut ids = vec!["p1".to_string()];
        assert!(!insert(&mut ids, "p1"));
        assert_eq!(ids, ["p1"]);
        assert!(insert(&mut ids, "p2"));
        assert_eq!(ids, ["p1", "p2"]);
    }

    #[test]
    fn remove_id_on_an_absent_id_is_a_no_op() {
        let mut ids = vec!["p1".to_string()];
        assert!(!remove_id(&mut ids, "gone"));
        assert_eq!(ids, ["p1"]);
        assert!(remove_id(&mut ids, "p1"));
        assert!(ids.is_empty());
    }
}

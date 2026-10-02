//! Checks Claude's package update feed against what is installed and installs the newer package.

use std::path::Path;

use super::registry;
use super::types::{Profile, Result};
use crate::platform::{self, ProcessTable};

#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeUpdateStatus {
    pub installed: String,
    pub latest: String,
    pub available: bool,
    pub outdated_profile_ids: Vec<String>,
}

pub fn status() -> Result<Option<ClaudeUpdateStatus>> {
    let Some(update) = platform::check_claude_package_update()? else {
        return Ok(None);
    };
    let plat = platform::current();
    let root = plat.profiles_root()?;
    let reg = registry::load()?;
    let table = ProcessTable::snapshot();

    let outdated_profile_ids = outdated_ids(&reg.profiles, |dir| {
        let dir = root.join(dir);
        matches!(plat.is_running_in(&table, &dir), Ok(Some(_)))
            && platform::runs_outdated_claude(&table, &dir)
    });

    Ok(Some(ClaudeUpdateStatus {
        installed: update.installed,
        latest: update.latest,
        available: update.available,
        outdated_profile_ids,
    }))
}

pub fn install() -> Result<()> {
    platform::install_claude_package_update()
}

fn outdated_ids(profiles: &[Profile], is_outdated: impl Fn(&Path) -> bool) -> Vec<String> {
    profiles
        .iter()
        .filter(|profile| is_outdated(Path::new(&profile.dir)))
        .map(|profile| profile.id.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(id: &str, dir: &str) -> Profile {
        Profile {
            id: id.to_string(),
            name: id.to_string(),
            dir: dir.to_string(),
            created_at: String::new(),
            last_used_at: None,
        }
    }

    #[test]
    fn keeps_registry_order() {
        let profiles = [
            profile("b", "Claude-b"),
            profile("a", "Claude-a"),
            profile("c", "Claude-c"),
        ];
        assert_eq!(outdated_ids(&profiles, |_| true), ["b", "a", "c"]);
    }

    #[test]
    fn skips_profiles_that_are_not_outdated() {
        let profiles = [
            profile("a", "Claude-a"),
            profile("b", "Claude-b"),
            profile("c", "Claude-c"),
        ];
        let ids = outdated_ids(&profiles, |dir| dir != Path::new("Claude-b"));
        assert_eq!(ids, ["a", "c"]);
    }

    #[test]
    fn no_profiles_gives_empty() {
        assert!(outdated_ids(&[], |_| true).is_empty());
    }
}

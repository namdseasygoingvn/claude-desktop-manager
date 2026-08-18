use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use super::registry;
use super::types::{CdmError, Registry, Result};
use crate::platform;

pub mod links;
pub mod membership;
pub mod merge;

/// S1's pool location. The only code that computes this path (00-overview, S1).
pub const POOL_DIR_NAME: &str = "session-pool";

/// `<manager_data_dir>/session-pool/`. One-liner join mirroring `registry.rs:23-25`
/// and `settings.rs:46-48`. Computes a path; never creates anything on disk.
pub fn pool_root() -> Result<PathBuf> {
    Ok(platform::current().manager_data_dir()?.join(POOL_DIR_NAME))
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JoinReport {
    // account-uuid dir names left untouched because they link somewhere else
    pub skipped_foreign: Vec<String>,
}

/// Links every account-uuid dir under the profile's `claude-code-sessions/` to the shared pool,
/// merging prior content in first (D7), and records the profile as a member. Refuses while the
/// profile is running (S5). Membership is written last, so a crash mid-join leaves every
/// already-linked account dir as `OurLink` and a re-run only retries the membership write.
pub fn join(profile_id: &str) -> Result<JoinReport> {
    let plat = platform::current();
    let root = plat.profiles_root()?;
    let reg = registry::load()?;
    let idx = index_of(&reg, profile_id)?;
    let profile_dir = root.join(&reg.profiles[idx].dir);

    if plat.is_running(&profile_dir)?.is_some() {
        return Err(CdmError::ProfileRunning(reg.profiles[idx].name.clone()));
    }
    if membership::is_member(profile_id) {
        return Ok(JoinReport::default());
    }

    let pool = pool_root()?;
    let report = link_profile(&profile_dir, &pool)?;

    membership::add(profile_id)?;
    Ok(report)
}

/// The membership read, re-exported at the orchestration boundary so callers (plan 11) never
/// reach into `membership.rs` directly. Infallible: empty on a missing store.
pub fn status() -> Vec<String> {
    membership::load().profile_ids
}

/// Reverts every account-uuid dir this profile had linked into the pool back to a real,
/// independent copy of the pool's content at the moment of the call (D3), then drops the
/// profile from membership. Refuses while the profile is running (S5); a no-op on a profile
/// that isn't a member. Membership is removed last, so a crash mid-leave leaves the
/// already-materialized dirs to be skipped on retry and only the membership write to redo.
pub fn leave(profile_id: &str) -> Result<()> {
    let plat = platform::current();
    let root = plat.profiles_root()?;
    let reg = registry::load()?;
    let idx = index_of(&reg, profile_id)?;
    let profile_dir = root.join(&reg.profiles[idx].dir);

    if plat.is_running(&profile_dir)?.is_some() {
        return Err(CdmError::ProfileRunning(reg.profiles[idx].name.clone()));
    }
    if !membership::is_member(profile_id) {
        return Ok(());
    }

    let pool = pool_root()?;
    for (account_uuid, state) in links::survey(&profile_dir, &pool) {
        if state != links::LinkState::OurLink {
            continue;
        }
        let account_dir = profile_dir.join(links::SESSIONS_DIR_NAME).join(&account_uuid);
        links::materialize(&account_dir, &pool)?;
    }

    membership::remove(profile_id)
}

/// At-launch pass (S5): brings each account-uuid dir under `profile_dir/claude-code-sessions/`
/// into the state this profile's current membership implies. Never blocks or fails the launch:
/// a per-directory error is logged and skipped, one bad sibling never stopping the rest.
pub fn reconcile(profile_id: &str, profile_dir: &Path) -> Result<()> {
    let Ok(pool) = pool_root() else {
        return Ok(());
    };
    let member = membership::is_member(profile_id);

    for (account_uuid, state) in links::survey(profile_dir, &pool) {
        let account_dir = profile_dir.join(links::SESSIONS_DIR_NAME).join(&account_uuid);
        if let Err(e) = reconcile_one(member, state, &account_dir, &pool) {
            log::warn!("session-pool reconcile failed for {}: {e}", account_dir.display());
        }
    }
    Ok(())
}

fn reconcile_one(member: bool, state: links::LinkState, account_dir: &Path, pool: &Path) -> Result<()> {
    match (member, state) {
        (true, links::LinkState::RealDir) => links::absorb(account_dir, pool),
        (false, links::LinkState::OurLink) => links::materialize(account_dir, pool),
        _ => Ok(()),
    }
}

/// Links every account-uuid dir under `profile_dir/claude-code-sessions/` to `pool`, merging
/// prior content in first (D7). `ForeignLink` entries are left untouched and named in the
/// report; `OurLink` entries are already done and are skipped, which is what makes a second
/// call safe to retry.
fn link_profile(profile_dir: &Path, pool: &Path) -> Result<JoinReport> {
    fs::create_dir_all(pool).map_err(|e| CdmError::Io(format!("create {}: {e}", pool.display())))?;

    let mut report = JoinReport::default();
    for (account_uuid, state) in links::survey(profile_dir, pool) {
        match state {
            links::LinkState::ForeignLink => report.skipped_foreign.push(account_uuid),
            links::LinkState::OurLink => {}
            links::LinkState::RealDir => {
                let account_dir = profile_dir.join(links::SESSIONS_DIR_NAME).join(&account_uuid);
                links::absorb(&account_dir, pool)?;
            }
        }
    }
    Ok(report)
}

fn index_of(reg: &Registry, id: &str) -> Result<usize> {
    reg.profiles
        .iter()
        .position(|p| p.id == id)
        .ok_or_else(|| CdmError::ProfileNotFound(id.to_string()))
}

// `platform::current()` resolves `HOME`/`APPDATA` for every path, so exercising `join`/`leave`
// end to end needs that env var pinned to a scratch dir for the call's duration. Shared by every
// HOME-pinning test module in the crate, not just one: two tests racing the real `HOME` var is a
// bug whether they come from the same `mod tests` or different ones.
#[cfg(test)]
pub(crate) mod home_guard {
    use std::path::Path;
    use std::sync::Mutex;

    static HOME_GUARD: Mutex<()> = Mutex::new(());

    #[cfg(unix)]
    pub(crate) fn with_home<R>(home: &Path, f: impl FnOnce() -> R) -> R {
        let _guard = HOME_GUARD.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let prev = std::env::var_os("HOME");
        std::env::set_var("HOME", home);
        let result = f();
        match prev {
            Some(value) => std::env::set_var("HOME", value),
            None => std::env::remove_var("HOME"),
        }
        result
    }
}

#[cfg(test)]
mod test_support {
    use super::*;
    #[cfg(unix)]
    use super::super::types::Profile;

    pub(super) fn account_dir(profile_dir: &Path, account_uuid: &str) -> PathBuf {
        profile_dir.join(links::SESSIONS_DIR_NAME).join(account_uuid)
    }

    pub(super) fn seed_session(profile_dir: &Path, account_uuid: &str, sub_uuid: &str, file: &str) {
        let dir = account_dir(profile_dir, account_uuid).join(sub_uuid);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(file), b"{}").unwrap();
    }

    #[cfg(unix)]
    pub(super) fn seed_profile(id: &str, dir: &str) -> PathBuf {
        let mut reg = Registry::default();
        reg.profiles.push(Profile {
            id: id.to_string(),
            name: id.to_string(),
            dir: dir.to_string(),
            created_at: String::new(),
            last_used_at: None,
        });
        registry::save(&reg).unwrap();
        platform::current().profiles_root().unwrap().join(dir)
    }
}

#[cfg(test)]
mod join_tests {
    use super::*;
    #[cfg(unix)]
    use super::home_guard::with_home;
    #[cfg(unix)]
    use super::test_support::seed_profile;
    use super::test_support::{account_dir, seed_session};

    #[test]
    fn a_real_account_dir_is_merged_into_the_pool_and_linked() {
        let profile = tempfile::tempdir().unwrap();
        let pool = tempfile::tempdir().unwrap();
        seed_session(profile.path(), "acct-1", "sub-1", "local_a.json");

        let report = link_profile(profile.path(), pool.path()).unwrap();

        assert!(report.skipped_foreign.is_empty());
        assert!(pool.path().join("sub-1").join("local_a.json").exists());
        assert_eq!(
            platform::current().link_target(&account_dir(profile.path(), "acct-1")),
            Some(pool.path().to_path_buf())
        );
    }

    #[test]
    fn a_foreign_link_is_skipped_and_reported_while_the_rest_still_link() {
        let profile = tempfile::tempdir().unwrap();
        let pool = tempfile::tempdir().unwrap();
        let elsewhere = tempfile::tempdir().unwrap();
        seed_session(profile.path(), "acct-real", "sub-1", "local_a.json");
        let foreign = account_dir(profile.path(), "acct-foreign");
        fs::create_dir_all(foreign.parent().unwrap()).unwrap();
        platform::current().link_dir(elsewhere.path(), &foreign).unwrap();

        let report = link_profile(profile.path(), pool.path()).unwrap();

        assert_eq!(report.skipped_foreign, vec!["acct-foreign".to_string()]);
        assert_eq!(
            platform::current().link_target(&foreign),
            Some(elsewhere.path().to_path_buf())
        );
        assert!(pool.path().join("sub-1").join("local_a.json").exists());
    }

    #[test]
    fn two_profiles_with_different_accounts_merge_into_one_pool_without_losing_either() {
        let profile_a = tempfile::tempdir().unwrap();
        let profile_b = tempfile::tempdir().unwrap();
        let pool = tempfile::tempdir().unwrap();
        seed_session(profile_a.path(), "acct-a", "sub-a", "local_a.json");
        seed_session(profile_b.path(), "acct-b", "sub-b", "local_b.json");

        link_profile(profile_a.path(), pool.path()).unwrap();
        link_profile(profile_b.path(), pool.path()).unwrap();

        assert!(pool.path().join("sub-a").join("local_a.json").exists());
        assert!(pool.path().join("sub-b").join("local_b.json").exists());
    }

    #[test]
    fn a_second_call_over_an_already_linked_account_dir_is_a_no_op() {
        let profile = tempfile::tempdir().unwrap();
        let pool = tempfile::tempdir().unwrap();
        seed_session(profile.path(), "acct-1", "sub-1", "local_a.json");
        link_profile(profile.path(), pool.path()).unwrap();

        let report = link_profile(profile.path(), pool.path()).unwrap();

        assert!(report.skipped_foreign.is_empty());
        assert_eq!(
            platform::current().link_target(&account_dir(profile.path(), "acct-1")),
            Some(pool.path().to_path_buf())
        );
    }

    #[test]
    #[cfg(unix)]
    fn join_resolves_the_profile_links_its_sessions_and_records_membership() {
        let home = tempfile::tempdir().unwrap();
        with_home(home.path(), || {
            let profile_dir = seed_profile("p1", "Claude-Test");
            seed_session(&profile_dir, "acct-1", "sub-1", "local_a.json");

            let report = join("p1").unwrap();

            assert!(report.skipped_foreign.is_empty());
            assert!(membership::is_member("p1"));
            assert!(pool_root().unwrap().join("sub-1").join("local_a.json").exists());
        });
    }

    #[test]
    #[cfg(unix)]
    fn joining_again_after_the_membership_write_was_lost_only_redoes_that_write() {
        let home = tempfile::tempdir().unwrap();
        with_home(home.path(), || {
            let profile_dir = seed_profile("p1", "Claude-Test");
            seed_session(&profile_dir, "acct-1", "sub-1", "local_a.json");
            join("p1").unwrap();

            // Simulate a crash before step 7 landed: the link swap happened, membership did not.
            membership::remove("p1").unwrap();

            let report = join("p1").unwrap();

            assert!(report.skipped_foreign.is_empty());
            assert!(membership::is_member("p1"));
        });
    }

    #[test]
    #[cfg(unix)]
    fn joining_an_already_recorded_member_is_a_no_op() {
        let home = tempfile::tempdir().unwrap();
        with_home(home.path(), || {
            let profile_dir = seed_profile("p1", "Claude-Test");
            seed_session(&profile_dir, "acct-1", "sub-1", "local_a.json");
            join("p1").unwrap();

            let report = join("p1").unwrap();

            assert!(report.skipped_foreign.is_empty());
        });
    }

    #[test]
    #[cfg(unix)]
    fn joining_an_unknown_profile_id_fails() {
        let home = tempfile::tempdir().unwrap();
        with_home(home.path(), || {
            assert!(matches!(join("does-not-exist"), Err(CdmError::ProfileNotFound(_))));
        });
    }
}

#[cfg(test)]
mod leave_tests {
    use super::*;
    #[cfg(unix)]
    use super::home_guard::with_home;
    #[cfg(unix)]
    use super::test_support::{account_dir, seed_profile, seed_session};

    #[test]
    #[cfg(unix)]
    fn leaving_turns_the_linked_account_dir_into_an_independent_copy() {
        let home = tempfile::tempdir().unwrap();
        with_home(home.path(), || {
            let profile_dir = seed_profile("p1", "Claude-Test");
            seed_session(&profile_dir, "acct-1", "sub-1", "local_a.json");
            join("p1").unwrap();
            let account = account_dir(&profile_dir, "acct-1");

            leave("p1").unwrap();

            assert!(platform::current().link_target(&account).is_none());
            assert!(account.join("sub-1").join("local_a.json").exists());
            assert!(!membership::is_member("p1"));
        });
    }

    #[test]
    #[cfg(unix)]
    fn a_former_member_keeps_the_snapshot_it_took_on_the_way_out() {
        let home = tempfile::tempdir().unwrap();
        with_home(home.path(), || {
            let profile_dir = seed_profile("p1", "Claude-Test");
            seed_session(&profile_dir, "acct-1", "sub-1", "local_a.json");
            join("p1").unwrap();
            let account = account_dir(&profile_dir, "acct-1");

            leave("p1").unwrap();
            let pool = pool_root().unwrap();
            fs::create_dir_all(pool.join("sub-2")).unwrap();
            fs::write(pool.join("sub-2").join("local_b.json"), b"{}").unwrap();

            assert!(!account.join("sub-2").exists());
        });
    }

    #[test]
    #[cfg(unix)]
    fn leaving_a_profile_never_joined_is_a_no_op() {
        let home = tempfile::tempdir().unwrap();
        with_home(home.path(), || {
            seed_profile("p1", "Claude-Test");

            leave("p1").unwrap();

            assert!(!membership::is_member("p1"));
            assert!(!pool_root().unwrap().exists());
        });
    }

    #[test]
    #[cfg(unix)]
    fn a_crash_before_membership_removal_is_retried_without_recopying() {
        let home = tempfile::tempdir().unwrap();
        with_home(home.path(), || {
            let profile_dir = seed_profile("p1", "Claude-Test");
            seed_session(&profile_dir, "acct-1", "sub-1", "local_a.json");
            join("p1").unwrap();
            let account = account_dir(&profile_dir, "acct-1");

            // Simulate a crash between step 4 (materialize) and step 5 (membership::remove).
            links::materialize(&account, &pool_root().unwrap()).unwrap();
            fs::write(account.join("marker"), b"kept").unwrap();

            leave("p1").unwrap();

            assert!(!membership::is_member("p1"));
            assert!(account.join("marker").exists());
        });
    }
}

#[cfg(test)]
mod reconcile_tests {
    use super::*;
    #[cfg(unix)]
    use super::home_guard::with_home;
    #[cfg(unix)]
    use super::test_support::{account_dir, seed_profile, seed_session};

    #[test]
    #[cfg(unix)]
    fn member_with_our_link_is_left_alone() {
        let home = tempfile::tempdir().unwrap();
        with_home(home.path(), || {
            let profile_dir = seed_profile("p1", "Claude-Test");
            seed_session(&profile_dir, "acct-1", "sub-1", "local_a.json");
            join("p1").unwrap();
            let account = account_dir(&profile_dir, "acct-1");
            let before = fs::symlink_metadata(&account).unwrap().modified().unwrap();

            reconcile("p1", &profile_dir).unwrap();

            assert_eq!(fs::symlink_metadata(&account).unwrap().modified().unwrap(), before);
            assert_eq!(platform::current().link_target(&account), Some(pool_root().unwrap()));
        });
    }

    #[test]
    #[cfg(unix)]
    fn member_with_a_real_dir_is_absorbed_and_relinked() {
        let home = tempfile::tempdir().unwrap();
        with_home(home.path(), || {
            let profile_dir = seed_profile("p1", "Claude-Test");
            seed_session(&profile_dir, "acct-1", "sub-1", "local_a.json");
            join("p1").unwrap();
            let account = account_dir(&profile_dir, "acct-1");
            // The app recreated a real dir over our link (Q8): remove the link, write fresh.
            fs::remove_file(&account).unwrap();
            seed_session(&profile_dir, "acct-1", "sub-2", "local_b.json");

            reconcile("p1", &profile_dir).unwrap();

            let pool = pool_root().unwrap();
            assert_eq!(platform::current().link_target(&account), Some(pool.clone()));
            assert!(pool.join("sub-1").join("local_a.json").exists());
            assert!(pool.join("sub-2").join("local_b.json").exists());
        });
    }

    #[test]
    #[cfg(unix)]
    fn member_with_a_foreign_link_is_never_touched() {
        let home = tempfile::tempdir().unwrap();
        with_home(home.path(), || {
            let profile_dir = seed_profile("p1", "Claude-Test");
            let elsewhere = tempfile::tempdir().unwrap();
            let account = account_dir(&profile_dir, "acct-foreign");
            fs::create_dir_all(account.parent().unwrap()).unwrap();
            platform::current().link_dir(elsewhere.path(), &account).unwrap();
            join("p1").unwrap();

            reconcile("p1", &profile_dir).unwrap();

            assert_eq!(
                platform::current().link_target(&account),
                Some(elsewhere.path().to_path_buf())
            );
        });
    }

    #[test]
    #[cfg(unix)]
    fn non_member_with_our_link_is_materialized() {
        let home = tempfile::tempdir().unwrap();
        with_home(home.path(), || {
            let profile_dir = seed_profile("p1", "Claude-Test");
            let pool = pool_root().unwrap();
            fs::create_dir_all(pool.join("sub-1")).unwrap();
            fs::write(pool.join("sub-1").join("local_a.json"), b"{}").unwrap();
            let account = account_dir(&profile_dir, "acct-1");
            fs::create_dir_all(account.parent().unwrap()).unwrap();
            platform::current().link_dir(&pool, &account).unwrap();

            reconcile("p1", &profile_dir).unwrap();

            assert!(platform::current().link_target(&account).is_none());
            assert!(account.join("sub-1").join("local_a.json").exists());
            assert!(!membership::is_member("p1"));
        });
    }

    #[test]
    #[cfg(unix)]
    fn non_member_with_a_real_dir_is_left_alone() {
        let home = tempfile::tempdir().unwrap();
        with_home(home.path(), || {
            let profile_dir = seed_profile("p1", "Claude-Test");
            seed_session(&profile_dir, "acct-1", "sub-1", "local_a.json");
            let account = account_dir(&profile_dir, "acct-1");

            reconcile("p1", &profile_dir).unwrap();

            assert!(platform::current().link_target(&account).is_none());
            assert!(account.join("sub-1").join("local_a.json").exists());
            assert!(!pool_root().unwrap().exists());
        });
    }

    #[test]
    #[cfg(unix)]
    fn non_member_with_a_foreign_link_is_never_touched() {
        let home = tempfile::tempdir().unwrap();
        with_home(home.path(), || {
            let profile_dir = seed_profile("p1", "Claude-Test");
            let elsewhere = tempfile::tempdir().unwrap();
            let account = account_dir(&profile_dir, "acct-1");
            fs::create_dir_all(account.parent().unwrap()).unwrap();
            platform::current().link_dir(elsewhere.path(), &account).unwrap();

            reconcile("p1", &profile_dir).unwrap();

            assert_eq!(
                platform::current().link_target(&account),
                Some(elsewhere.path().to_path_buf())
            );
        });
    }

    #[test]
    #[cfg(unix)]
    fn a_failing_sibling_never_stops_the_rest_and_reconcile_still_returns_ok() {
        let home = tempfile::tempdir().unwrap();
        with_home(home.path(), || {
            let profile_dir = seed_profile("p1", "Claude-Test");
            seed_session(&profile_dir, "acct-1", "sub-1", "local_a.json");
            seed_session(&profile_dir, "acct-2", "sub-2", "local_b.json");
            membership::add("p1").unwrap();
            // A file where absorb expects to create the pool dir: every RealDir sibling fails.
            fs::write(pool_root().unwrap(), b"not a directory").unwrap();

            let result = reconcile("p1", &profile_dir);

            assert!(result.is_ok());
            assert!(account_dir(&profile_dir, "acct-1").is_dir());
            assert!(account_dir(&profile_dir, "acct-2").is_dir());
        });
    }

    #[test]
    #[cfg(unix)]
    fn a_second_call_with_no_disk_change_mutates_nothing() {
        let home = tempfile::tempdir().unwrap();
        with_home(home.path(), || {
            let profile_dir = seed_profile("p1", "Claude-Test");
            seed_session(&profile_dir, "acct-1", "sub-1", "local_a.json");
            membership::add("p1").unwrap();
            let account = account_dir(&profile_dir, "acct-1");

            reconcile("p1", &profile_dir).unwrap();
            let after_first = fs::symlink_metadata(&account).unwrap().modified().unwrap();

            reconcile("p1", &profile_dir).unwrap();

            assert_eq!(fs::symlink_metadata(&account).unwrap().modified().unwrap(), after_first);
        });
    }
}

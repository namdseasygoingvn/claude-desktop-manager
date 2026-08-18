use std::fs;
use std::path::{Path, PathBuf};

use super::super::naming;
use super::super::profile::random_id;
use super::super::types::{CdmError, Result};
use super::merge;
use crate::platform;

/// D1's scope, defined once; callers join this onto a profile dir, never hardcode the string.
pub const SESSIONS_DIR_NAME: &str = "claude-code-sessions";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkState {
    RealDir,
    OurLink,
    ForeignLink,
}

/// `profile_dir` is a profile's root (e.g. `<profiles_root>/Claude-Work`), not the sessions tree
/// itself. `pool` is the already-resolved pool directory (S1). Returns one row per account-uuid
/// entry found, sorted by name; `Vec::new()` — never `Err` — when `claude-code-sessions` is
/// missing or empty, mirroring `claude_code::versions`'s missing-store behavior.
pub fn survey(profile_dir: &Path, pool: &Path) -> Vec<(String, LinkState)> {
    let dir = profile_dir.join(SESSIONS_DIR_NAME);
    let Ok(entries) = fs::read_dir(&dir) else {
        return Vec::new();
    };

    let mut rows: Vec<(String, LinkState)> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            if name.starts_with('.') {
                return None;
            }
            classify(&entry.path(), pool).map(|state| (name, state))
        })
        .collect();

    rows.sort_by(|a, b| a.0.cmp(&b.0));
    rows
}

/// Never follows the link: an entry that is neither a directory nor a link (a plain file) has no
/// state and is dropped by the caller.
fn classify(path: &Path, pool: &Path) -> Option<LinkState> {
    match platform::current().link_target(path) {
        Some(target) => Some(if same_path(&target, pool) {
            LinkState::OurLink
        } else {
            LinkState::ForeignLink
        }),
        None if path.is_dir() => Some(LinkState::RealDir),
        None => None,
    }
}

fn same_path(a: &Path, b: &Path) -> bool {
    naming::normalize_key(&a.to_string_lossy()) == naming::normalize_key(&b.to_string_lossy())
}

/// Precondition: `account_dir` classifies as `LinkState::RealDir`. Postcondition: it classifies
/// as `LinkState::OurLink` targeting `pool`, which holds the union of its own and `account_dir`'s
/// prior contents (05's merge rule). Creates `pool` first, so this is also the guarantee plan 09
/// leans on when the pool does not exist yet.
pub(crate) fn absorb(account_dir: &Path, pool: &Path) -> Result<()> {
    fs::create_dir_all(pool).map_err(|e| CdmError::Io(format!("create {}: {e}", pool.display())))?;
    let plan = merge::plan(account_dir, pool);
    let outcome = merge::apply(account_dir, pool, &plan);
    for path in &plan.unreadable {
        log::warn!("session-pool merge skipped unreadable {}", path.display());
    }
    for (path, error) in &outcome.failed {
        log::warn!("session-pool merge failed to copy {}: {error}", path.display());
    }
    log::debug!("session-pool merged {} file(s) from {}", outcome.copied.len(), account_dir.display());

    let retired = dotted_sibling(account_dir, "retired");
    fs::rename(account_dir, &retired)
        .map_err(|e| CdmError::Io(format!("retire {}: {e}", account_dir.display())))?;

    if let Err(e) = platform::current().link_dir(pool, account_dir) {
        let _ = fs::rename(&retired, account_dir);
        return Err(e);
    }

    let _ = fs::remove_dir_all(&retired);
    Ok(())
}

/// Precondition: `account_dir` classifies as `LinkState::OurLink`. Postcondition: `account_dir`
/// is a real directory holding a copy of the pool root's content at call time. Same stage/
/// retire/place shape as `swap_for_clone` (`claude_code.rs:89-112`), with the real/link roles
/// swapped from `absorb`.
pub(crate) fn materialize(account_dir: &Path, pool: &Path) -> Result<()> {
    let staged = dotted_sibling(account_dir, "staged");
    if let Err(e) = platform::current().clone_tree(pool, &staged) {
        let _ = fs::remove_dir_all(&staged);
        return Err(e);
    }

    let retired = dotted_sibling(account_dir, "retired");
    if let Err(e) = fs::rename(account_dir, &retired) {
        let _ = fs::remove_dir_all(&staged);
        return Err(CdmError::Io(format!("retire {}: {e}", account_dir.display())));
    }
    if let Err(e) = fs::rename(&staged, account_dir) {
        let _ = fs::rename(&retired, account_dir);
        let _ = fs::remove_dir_all(&staged);
        return Err(CdmError::Io(format!("place {}: {e}", account_dir.display())));
    }

    let _ = fs::remove_dir_all(&retired);
    Ok(())
}

/// Dotted, so `survey` never mistakes a half-swapped dir for a live account dir
/// (`claude_code.rs:148-151` staging-name idiom); `tag` keeps the stage/retire pair a single
/// swap needs from ever drawing the same sibling name.
fn dotted_sibling(path: &Path, tag: &str) -> PathBuf {
    let name = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
    path.with_file_name(format!(".cdm-{tag}-{name}-{}", random_id()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sessions_dir(profile_dir: &Path) -> std::path::PathBuf {
        let dir = profile_dir.join(SESSIONS_DIR_NAME);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_real_directory_is_reported_as_real_dir() {
        let profile = tempfile::tempdir().unwrap();
        let pool = tempfile::tempdir().unwrap();
        fs::create_dir(sessions_dir(profile.path()).join("acct-1")).unwrap();

        assert_eq!(
            survey(profile.path(), pool.path()),
            vec![("acct-1".to_string(), LinkState::RealDir)]
        );
    }

    #[test]
    fn a_link_to_the_pool_is_our_link() {
        let profile = tempfile::tempdir().unwrap();
        let pool = tempfile::tempdir().unwrap();
        let link = sessions_dir(profile.path()).join("acct-1");
        platform::current().link_dir(pool.path(), &link).unwrap();

        assert_eq!(
            survey(profile.path(), pool.path()),
            vec![("acct-1".to_string(), LinkState::OurLink)]
        );
    }

    #[test]
    fn a_link_to_anywhere_else_is_foreign() {
        let profile = tempfile::tempdir().unwrap();
        let pool = tempfile::tempdir().unwrap();
        let elsewhere = tempfile::tempdir().unwrap();
        let link = sessions_dir(profile.path()).join("acct-1");
        platform::current().link_dir(elsewhere.path(), &link).unwrap();

        assert_eq!(
            survey(profile.path(), pool.path()),
            vec![("acct-1".to_string(), LinkState::ForeignLink)]
        );
    }

    #[test]
    fn a_dangling_our_link_is_still_classified_from_the_recorded_target() {
        let profile = tempfile::tempdir().unwrap();
        let pool = tempfile::tempdir().unwrap();
        let pool_path = pool.path().to_path_buf();
        let link = sessions_dir(profile.path()).join("acct-1");
        platform::current().link_dir(&pool_path, &link).unwrap();
        drop(pool);

        assert_eq!(
            survey(profile.path(), &pool_path),
            vec![("acct-1".to_string(), LinkState::OurLink)]
        );
    }

    #[test]
    fn a_missing_sessions_tree_is_an_empty_survey_not_an_error() {
        let profile = tempfile::tempdir().unwrap();
        let pool = tempfile::tempdir().unwrap();

        assert!(survey(profile.path(), pool.path()).is_empty());
    }

    #[test]
    fn an_empty_sessions_tree_is_an_empty_survey() {
        let profile = tempfile::tempdir().unwrap();
        let pool = tempfile::tempdir().unwrap();
        sessions_dir(profile.path());

        assert!(survey(profile.path(), pool.path()).is_empty());
    }

    #[test]
    fn a_plain_file_in_place_of_an_account_dir_is_silently_excluded() {
        let profile = tempfile::tempdir().unwrap();
        let pool = tempfile::tempdir().unwrap();
        fs::write(sessions_dir(profile.path()).join("acct-1"), "not a dir").unwrap();

        assert!(survey(profile.path(), pool.path()).is_empty());
    }

    #[test]
    fn a_case_differing_target_still_matches_as_our_link() {
        let profile = tempfile::tempdir().unwrap();
        let pool = tempfile::tempdir().unwrap();
        let upper = pool.path().to_string_lossy().to_uppercase();
        let link = sessions_dir(profile.path()).join("acct-1");
        platform::current().link_dir(Path::new(&upper), &link).unwrap();

        assert_eq!(
            survey(profile.path(), pool.path()),
            vec![("acct-1".to_string(), LinkState::OurLink)]
        );
    }

    #[test]
    #[cfg(unix)]
    fn a_symlink_whose_target_never_existed_is_still_one_row_not_zero() {
        let profile = tempfile::tempdir().unwrap();
        let pool = tempfile::tempdir().unwrap();
        let link = sessions_dir(profile.path()).join("x");
        std::os::unix::fs::symlink("/does/not/exist", &link).unwrap();

        assert_eq!(
            survey(profile.path(), pool.path()),
            vec![("x".to_string(), LinkState::ForeignLink)]
        );
    }

    #[test]
    fn a_dotted_entry_is_excluded_like_claude_codes_own_filter() {
        let profile = tempfile::tempdir().unwrap();
        let pool = tempfile::tempdir().unwrap();
        fs::create_dir(sessions_dir(profile.path()).join(".DS_Store")).unwrap();

        assert!(survey(profile.path(), pool.path()).is_empty());
    }

    #[test]
    fn absorb_moves_content_into_an_empty_pool_and_links_the_account_dir() {
        let profile = tempfile::tempdir().unwrap();
        let pool = tempfile::tempdir().unwrap();
        let account = sessions_dir(profile.path()).join("acct-1");
        fs::create_dir_all(account.join("sub-1")).unwrap();
        fs::write(account.join("sub-1").join("local_a.json"), b"{}").unwrap();

        absorb(&account, pool.path()).unwrap();

        assert!(pool.path().join("sub-1").join("local_a.json").exists());
        assert_eq!(
            survey(profile.path(), pool.path()),
            vec![("acct-1".to_string(), LinkState::OurLink)]
        );
    }

    #[test]
    fn absorb_merges_into_a_pool_that_already_holds_another_members_content() {
        let profile = tempfile::tempdir().unwrap();
        let pool = tempfile::tempdir().unwrap();
        fs::create_dir_all(pool.path().join("sub-existing")).unwrap();
        fs::write(pool.path().join("sub-existing").join("local_other.json"), b"{}").unwrap();
        let account = sessions_dir(profile.path()).join("acct-1");
        fs::create_dir_all(account.join("sub-1")).unwrap();
        fs::write(account.join("sub-1").join("local_a.json"), b"{}").unwrap();

        absorb(&account, pool.path()).unwrap();

        assert!(pool.path().join("sub-existing").join("local_other.json").exists());
        assert!(pool.path().join("sub-1").join("local_a.json").exists());
    }

    #[test]
    fn materialize_turns_a_linked_account_dir_into_a_real_copy_of_the_pool() {
        let profile = tempfile::tempdir().unwrap();
        let pool = tempfile::tempdir().unwrap();
        fs::create_dir_all(pool.path().join("sub-1")).unwrap();
        fs::write(pool.path().join("sub-1").join("local_a.json"), b"{}").unwrap();
        let account = sessions_dir(profile.path()).join("acct-1");
        platform::current().link_dir(pool.path(), &account).unwrap();

        materialize(&account, pool.path()).unwrap();

        assert!(platform::current().link_target(&account).is_none());
        assert!(account.join("sub-1").join("local_a.json").exists());
    }

    #[test]
    fn materialize_leaves_the_link_untouched_when_the_pool_is_gone() {
        let profile = tempfile::tempdir().unwrap();
        let pool = tempfile::tempdir().unwrap();
        let pool_path = pool.path().to_path_buf();
        let account = sessions_dir(profile.path()).join("acct-1");
        platform::current().link_dir(&pool_path, &account).unwrap();
        drop(pool);

        assert!(materialize(&account, &pool_path).is_err());
        assert_eq!(platform::current().link_target(&account), Some(pool_path));
    }
}

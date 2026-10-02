//! Merges files a packaged Claude wrote into the AppData virtualization overlay back into the real profile dir and the session pool.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::session_pool::links::SESSIONS_DIR_NAME;
use super::session_pool::{self, merge};
use crate::platform;

const SESSIONS_MERGED_DIR: &str = ".cdm-sessions-merged";
const RECOVERED_SUFFIX: &str = ".cdm-recovered-";

/// Brings files a packaged Claude wrote into the AppData overlay back into `profile_dir` and the
/// session pool. Call only while the profile is not running. Never fails the launch.
pub fn recover(profile_dir: &Path) {
    let Some(overlay) = platform::virtualized_copy_of(profile_dir).filter(|p| p.is_dir()) else {
        return;
    };
    let pool = match session_pool::pool_root() {
        Ok(pool) => pool,
        Err(e) => {
            log::warn!("overlay recovery skipped, no session pool path: {e}");
            return;
        }
    };
    let pool_overlay = platform::virtualized_copy_of(&pool).filter(|p| p.is_dir());

    if recover_from(&overlay, profile_dir, pool_overlay.as_deref(), &pool) {
        log::info!(
            "recovered overlay {} into {}",
            overlay.display(),
            profile_dir.display()
        );
    } else {
        log::warn!(
            "overlay {} only partly merged into {}; it will be retried next launch",
            overlay.display(),
            profile_dir.display()
        );
    }
}

pub(crate) fn recover_from(
    overlay: &Path,
    profile_dir: &Path,
    pool_overlay: Option<&Path>,
    pool: &Path,
) -> bool {
    let sessions_clean = merge_sessions(overlay, profile_dir);
    if sessions_clean {
        set_sessions_aside(overlay);
    }
    let mut clean = sessions_clean;
    clean &= merge_tree(overlay, profile_dir);
    if let Some(pool_overlay) = pool_overlay {
        clean &= merge_tree(pool_overlay, pool);
    }
    if !clean {
        return false;
    }

    let mut renamed = rename_aside(overlay);
    if let Some(pool_overlay) = pool_overlay {
        renamed &= rename_aside(pool_overlay);
    }
    renamed
}

fn merge_sessions(overlay: &Path, profile_dir: &Path) -> bool {
    let Ok(accounts) = fs::read_dir(overlay.join(SESSIONS_DIR_NAME)) else {
        return true;
    };
    let mut clean = true;
    for account in accounts.flatten() {
        if !account.file_type().is_ok_and(|t| t.is_dir()) {
            continue;
        }
        let plain = profile_dir
            .join(SESSIONS_DIR_NAME)
            .join(account.file_name());
        let dest = fs::canonicalize(&plain).unwrap_or(plain);
        clean &= merge_tree(&account.path(), &dest);
    }
    clean
}

fn set_sessions_aside(overlay: &Path) {
    let sessions = overlay.join(SESSIONS_DIR_NAME);
    if !sessions.exists() {
        return;
    }
    if let Err(e) = fs::rename(&sessions, overlay.join(SESSIONS_MERGED_DIR)) {
        log::warn!(
            "overlay sessions dir {} not set aside: {e}",
            sessions.display()
        );
    }
}

fn merge_tree(source: &Path, dest: &Path) -> bool {
    let plan = merge::plan(source, dest);
    for path in &plan.unreadable {
        log::warn!("overlay recovery skipped unreadable {}", path.display());
    }
    let outcome = merge::apply(source, dest, &plan);
    for (path, reason) in &outcome.failed {
        log::warn!(
            "overlay recovery could not copy {}: {reason}",
            path.display()
        );
    }
    outcome.failed.is_empty()
}

fn rename_aside(dir: &Path) -> bool {
    if !dir.exists() {
        return true;
    }
    let Some(name) = dir.file_name() else {
        return false;
    };
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let mut aside_name = name.to_os_string();
    aside_name.push(format!("{RECOVERED_SUFFIX}{secs}"));
    let aside: PathBuf = dir.with_file_name(aside_name);
    match fs::rename(dir, &aside) {
        Ok(()) => true,
        Err(e) => {
            log::warn!("overlay {} not renamed aside: {e}", dir.display());
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::time::Duration;

    const ACCOUNT: &str = "acct-1";

    fn write(path: &Path, contents: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, contents).unwrap();
    }

    fn set_mtime(path: &Path, time: SystemTime) {
        let file = File::options().write(true).open(path).unwrap();
        file.set_times(fs::FileTimes::new().set_modified(time))
            .unwrap();
    }

    struct Tree {
        _root: tempfile::TempDir,
        overlay: PathBuf,
        profile: PathBuf,
        pool: PathBuf,
    }

    fn tree() -> Tree {
        let root = tempfile::tempdir().unwrap();
        let overlay = root.path().join("overlay");
        let profile = root.path().join("Claude-Test");
        let pool = root.path().join("session-pool");
        for dir in [&overlay, &profile, &pool] {
            fs::create_dir_all(dir).unwrap();
        }
        Tree {
            _root: root,
            overlay,
            profile,
            pool,
        }
    }

    fn run(t: &Tree) -> bool {
        recover_from(&t.overlay, &t.profile, None, &t.pool)
    }

    fn recovered_sibling(dir: &Path) -> Option<PathBuf> {
        let prefix = format!(
            "{}{RECOVERED_SUFFIX}",
            dir.file_name().unwrap().to_string_lossy()
        );
        fs::read_dir(dir.parent().unwrap())
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .find(|p| {
                p.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with(&prefix)
            })
    }

    #[test]
    fn a_new_overlay_file_appears_in_the_profile_dir() {
        let t = tree();
        write(
            &t.overlay
                .join("Local Storage")
                .join("leveldb")
                .join("1.ldb"),
            "fresh",
        );

        assert!(run(&t));
        assert_eq!(
            fs::read_to_string(
                t.profile
                    .join("Local Storage")
                    .join("leveldb")
                    .join("1.ldb")
            )
            .unwrap(),
            "fresh"
        );
    }

    #[test]
    fn a_newer_overlay_file_replaces_an_older_real_one() {
        let t = tree();
        let now = SystemTime::now();
        write(&t.profile.join("config.json"), "old");
        set_mtime(
            &t.profile.join("config.json"),
            now - Duration::from_secs(100),
        );
        write(&t.overlay.join("config.json"), "new");
        set_mtime(&t.overlay.join("config.json"), now);

        assert!(run(&t));
        assert_eq!(
            fs::read_to_string(t.profile.join("config.json")).unwrap(),
            "new"
        );
    }

    #[test]
    fn an_older_overlay_file_does_not_replace_a_newer_real_one() {
        let t = tree();
        let now = SystemTime::now();
        write(&t.profile.join("config.json"), "real");
        set_mtime(&t.profile.join("config.json"), now);
        write(&t.overlay.join("config.json"), "stale");
        set_mtime(
            &t.overlay.join("config.json"),
            now - Duration::from_secs(100),
        );

        assert!(run(&t));
        assert_eq!(
            fs::read_to_string(t.profile.join("config.json")).unwrap(),
            "real"
        );
    }

    #[test]
    fn overlay_session_files_land_in_the_pool_when_the_account_dir_is_a_link() {
        let t = tree();
        let sessions = t.profile.join(SESSIONS_DIR_NAME);
        fs::create_dir_all(&sessions).unwrap();
        platform::current()
            .link_dir(&t.pool, &sessions.join(ACCOUNT))
            .unwrap();
        write(
            &t.overlay
                .join(SESSIONS_DIR_NAME)
                .join(ACCOUNT)
                .join("sub-1")
                .join("local_a.json"),
            "chat",
        );

        assert!(run(&t));
        assert_eq!(
            fs::read_to_string(t.pool.join("sub-1").join("local_a.json")).unwrap(),
            "chat"
        );
    }

    #[test]
    fn a_full_success_renames_the_overlay_aside() {
        let t = tree();
        write(&t.overlay.join("a.txt"), "a");
        write(
            &t.overlay
                .join(SESSIONS_DIR_NAME)
                .join(ACCOUNT)
                .join("local_a.json"),
            "chat",
        );

        assert!(run(&t));
        assert!(!t.overlay.exists());
        let aside = recovered_sibling(&t.overlay).expect("recovered sibling");
        assert!(aside.join(SESSIONS_MERGED_DIR).join(ACCOUNT).is_dir());
        assert!(t
            .profile
            .join(SESSIONS_DIR_NAME)
            .join(ACCOUNT)
            .join("local_a.json")
            .is_file());
    }

    #[test]
    fn a_second_run_with_no_overlay_is_a_no_op() {
        let t = tree();
        write(&t.overlay.join("a.txt"), "a");
        assert!(run(&t));
        let before = fs::read_dir(&t.profile).unwrap().count();

        assert!(recover_from(&t.overlay, &t.profile, None, &t.pool));
        assert_eq!(fs::read_dir(&t.profile).unwrap().count(), before);
        assert!(!t.overlay.exists());
    }

    #[test]
    fn a_pool_overlay_merges_into_the_pool() {
        let t = tree();
        let pool_overlay = t.overlay.with_file_name("pool-overlay");
        write(&pool_overlay.join("sub-9").join("local_z.json"), "pooled");

        assert!(recover_from(
            &t.overlay,
            &t.profile,
            Some(&pool_overlay),
            &t.pool
        ));
        assert_eq!(
            fs::read_to_string(t.pool.join("sub-9").join("local_z.json")).unwrap(),
            "pooled"
        );
        assert!(!pool_overlay.exists());
        assert!(recovered_sibling(&pool_overlay).is_some());
    }

    #[test]
    fn a_failed_copy_renames_nothing_and_reports_false() {
        let t = tree();
        write(&t.overlay.join("blocked").join("x.txt"), "x");
        write(&t.profile.join("blocked"), "a file where a dir must go");

        assert!(!run(&t));
        assert!(t.overlay.join("blocked").join("x.txt").is_file());
        assert!(recovered_sibling(&t.overlay).is_none());
    }

    #[test]
    fn a_failed_session_copy_keeps_the_sessions_dir_for_the_retry() {
        let t = tree();
        write(
            &t.overlay
                .join(SESSIONS_DIR_NAME)
                .join(ACCOUNT)
                .join("sub")
                .join("local_a.json"),
            "chat",
        );
        write(
            &t.profile.join(SESSIONS_DIR_NAME).join(ACCOUNT).join("sub"),
            "blocker",
        );

        assert!(!run(&t));
        assert!(t.overlay.join(SESSIONS_DIR_NAME).join(ACCOUNT).is_dir());
        assert!(!t.overlay.join(SESSIONS_MERGED_DIR).exists());
    }
}

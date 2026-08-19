//! One real copy of each claude-code runtime, handed to every profile as a filesystem clone.
//!
//! Claude Desktop downloads a ~260 MB runtime into `<profile>/claude-code/<version>/`, and it
//! does so per profile: six profiles meant six copies of identical bytes. The store keeps one
//! and clones it, so every profile after the first costs nothing.
//!
//! The store is the stock install's own `claude-code`, the one folder every machine already
//! has. cdm only ever *adds* a build there and never rewrites or removes one — the unmanaged
//! install stays the unmanaged install, and the default app remains the owner of its lifecycle.
//!
//! Nothing here is load-bearing for a launch. Every step is skippable, and a failure always
//! leaves the profile's own working copy in place.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use super::profile::{is_unmanaged_dir, random_id, UNMANAGED_DIR};
use super::types::{CdmError, Result};
use crate::platform;

pub const DIR_NAME: &str = "claude-code";

/// Written by Claude Desktop once it has checked the download, and identical across profiles
/// holding the same build. It is the app's own integrity record, so it is the only thing cdm
/// will accept as proof that two directories are interchangeable.
const VERIFIED_FILE: &str = ".verified";
/// Versions this profile already draws from the store, so a launch is not a re-clone.
const COLLAPSED_FILE: &str = ".cdm-shared";

/// Point one profile's runtimes at the shared store, seeding the store from this profile for
/// any build it has not seen. Call only while the profile is down.
pub fn sync(profile_dir: &Path) -> Result<()> {
    // The default install's claude-code dir IS the store; sharing it with itself would clone
    // the tree onto itself and remove the store's version dir mid-swap.
    let is_the_store_itself = profile_dir
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(is_unmanaged_dir);
    if is_the_store_itself {
        return Ok(());
    }

    let store = store_dir()?;
    let local = profile_dir.join(DIR_NAME);
    let known = collapsed(&local);
    // Rebuilt from what is on disk rather than extended, so a version the app has since deleted
    // and re-downloaded is collapsed again instead of being skipped by a stale entry.
    let mut current = HashSet::new();

    for version in versions(&local) {
        if matches!(share(&store, &local, &version, &known), Ok(true)) {
            current.insert(version);
        }
    }

    write_collapsed(&local, &current)
}

/// `Ok(true)` once this profile's copy is known to share the store's blocks. A build cdm cannot
/// vouch for is left alone and reported unshared, so a later launch reconsiders it — the app may
/// not have written `.verified` yet.
fn share(store: &Path, local: &Path, version: &str, collapsed: &HashSet<String>) -> Result<bool> {
    let shared = store.join(version);
    let mine = local.join(version);

    if !shared.is_dir() {
        fs::create_dir_all(store)
            .map_err(|e| CdmError::Io(format!("create {}: {e}", store.display())))?;
        // Seeded from this very copy, so the two already share every block.
        stage_into(&mine, &shared)?;
        return Ok(true);
    }
    if collapsed.contains(version) {
        return Ok(true);
    }
    if !same_build(&shared, &mine) {
        return Ok(false);
    }
    swap_for_clone(&shared, &mine)?;
    Ok(true)
}

/// Clone beside the target and rename into place, so an interrupted run can never leave a
/// half-written build where a whole one is expected.
fn stage_into(src: &Path, dst: &Path) -> Result<()> {
    let staged = staging(dst, "staged");
    let _ = fs::remove_dir_all(&staged);
    if let Err(e) = platform::current().clone_tree(src, &staged) {
        let _ = fs::remove_dir_all(&staged);
        return Err(e);
    }
    fs::rename(&staged, dst).map_err(|e| {
        let _ = fs::remove_dir_all(&staged);
        CdmError::Io(format!("place {}: {e}", dst.display()))
    })
}

fn swap_for_clone(shared: &Path, mine: &Path) -> Result<()> {
    // Distinct tags, never two draws of the same random name: colliding here would rename the
    // profile's only copy onto the clone meant to replace it.
    let staged = staging(mine, "staged");
    let retired = staging(mine, "retired");
    let _ = fs::remove_dir_all(&staged);
    if let Err(e) = platform::current().clone_tree(shared, &staged) {
        let _ = fs::remove_dir_all(&staged);
        return Err(e);
    }

    if let Err(e) = fs::rename(mine, &retired) {
        let _ = fs::remove_dir_all(&staged);
        return Err(CdmError::Io(format!("retire {}: {e}", mine.display())));
    }
    if let Err(e) = fs::rename(&staged, mine) {
        let _ = fs::rename(&retired, mine);
        let _ = fs::remove_dir_all(&staged);
        return Err(CdmError::Io(format!("place {}: {e}", mine.display())));
    }

    let _ = fs::remove_dir_all(&retired);
    Ok(())
}

fn same_build(a: &Path, b: &Path) -> bool {
    match (verified(a), verified(b)) {
        (Some(left), Some(right)) => left == right,
        _ => false,
    }
}

fn verified(dir: &Path) -> Option<String> {
    let hash = fs::read_to_string(dir.join(VERIFIED_FILE)).ok()?;
    let hash = hash.trim().to_string();
    (!hash.is_empty()).then_some(hash)
}

fn store_dir() -> Result<PathBuf> {
    Ok(platform::current()
        .profiles_root()?
        .join(UNMANAGED_DIR)
        .join(DIR_NAME))
}

/// Version directories only: the app also drops `.DS_Store` and cdm its own bookkeeping here.
fn versions(dir: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| !name.starts_with('.'))
        .collect()
}

/// Dotted, so `versions` never mistakes a half-built tree for a build the app could run.
fn staging(target: &Path, tag: &str) -> PathBuf {
    let name = target.file_name().unwrap_or_default().to_string_lossy().into_owned();
    target.with_file_name(format!(".cdm-{tag}-{name}-{}", random_id()))
}

fn collapsed(local: &Path) -> HashSet<String> {
    fs::read_to_string(local.join(COLLAPSED_FILE))
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect()
}

fn write_collapsed(local: &Path, versions: &HashSet<String>) -> Result<()> {
    if !local.is_dir() {
        return Ok(());
    }
    let mut sorted: Vec<&String> = versions.iter().collect();
    sorted.sort();
    let body: String = sorted.iter().map(|v| format!("{v}\n")).collect();
    let path = local.join(COLLAPSED_FILE);
    fs::write(&path, body).map_err(|e| CdmError::Io(format!("write {}: {e}", path.display())))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build(dir: &Path, hash: &str) {
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join(VERIFIED_FILE), hash).unwrap();
        fs::write(dir.join("payload"), "x").unwrap();
    }

    #[test]
    fn only_a_matching_verified_hash_makes_two_builds_interchangeable() {
        let root = tempfile::tempdir().unwrap();
        build(&root.path().join("a"), "abc");
        build(&root.path().join("b"), "abc");
        build(&root.path().join("c"), "def");
        assert!(same_build(&root.path().join("a"), &root.path().join("b")));
        assert!(!same_build(&root.path().join("a"), &root.path().join("c")));
    }

    #[test]
    fn a_build_without_a_verified_file_is_never_replaced() {
        let root = tempfile::tempdir().unwrap();
        build(&root.path().join("a"), "abc");
        fs::create_dir_all(root.path().join("bare")).unwrap();
        assert!(!same_build(&root.path().join("a"), &root.path().join("bare")));
    }

    #[test]
    fn dotted_entries_are_not_versions() {
        let root = tempfile::tempdir().unwrap();
        build(&root.path().join("2.1.221"), "abc");
        fs::create_dir_all(root.path().join(".cdm-staging")).unwrap();
        fs::write(root.path().join(".DS_Store"), "").unwrap();
        assert_eq!(versions(root.path()), vec!["2.1.221".to_string()]);
    }

    #[test]
    fn the_store_is_the_stock_installs_own_runtime_folder() {
        let store = store_dir().unwrap();
        assert!(store.ends_with(Path::new(UNMANAGED_DIR).join(DIR_NAME)));
    }

    #[test]
    fn sync_is_a_no_op_for_the_unmanaged_installs_own_dir() {
        let root = tempfile::tempdir().unwrap();
        let profile_dir = root.path().join(UNMANAGED_DIR);
        // No versions on purpose: a regressed guard then only writes the collapsed file locally
        // instead of seeding the machine's real store.
        fs::create_dir_all(profile_dir.join(DIR_NAME)).unwrap();

        sync(&profile_dir).unwrap();

        assert!(!profile_dir.join(DIR_NAME).join(COLLAPSED_FILE).exists());
    }
}

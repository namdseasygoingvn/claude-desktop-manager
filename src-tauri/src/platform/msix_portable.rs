use crate::core::types::{CdmError, Result};
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

const CACHE_DIR_NAME: &str = "msix-app";
const PARTIAL_SUFFIX: &str = ".partial";

/// Windows won't reliably launch the exe in place inside the package store (conditional ACLs,
/// no VFS overlay, no package identity for a bare CreateProcess), so this maintains a plain
/// copy of the payload elsewhere and hands back the exe inside it.
pub(super) fn launchable_copy(payload_exe: &Path) -> Result<PathBuf> {
    let full_name = super::msix::package_full_name(payload_exe).ok_or_else(|| {
        CdmError::Other(format!(
            "{} is not an MSIX package-store path",
            payload_exe.display()
        ))
    })?;
    let exe_name = payload_exe
        .file_name()
        .ok_or_else(|| CdmError::Other(format!("{} has no file name", payload_exe.display())))?;

    // LOCALAPPDATA, not APPDATA: this is a machine-local binary cache, not roaming user data.
    let cache_root = super::env_dir(super::win32::LOCAL_APP_DATA)?
        .join(super::MANAGER_DIR_NAME)
        .join(CACHE_DIR_NAME);
    let final_dir = cache_root.join(&full_name);
    let final_exe = final_dir.join(exe_name);

    if super::is_executable_file(&final_exe) {
        prune_stale(&cache_root, &full_name, exe_name);
        return Ok(final_exe);
    }

    let payload_dir = payload_exe.parent().ok_or_else(|| {
        CdmError::Other(format!("{} has no parent directory", payload_exe.display()))
    })?;
    let staging_dir = cache_root.join(format!("{full_name}{PARTIAL_SUFFIX}"));

    let _ = fs::remove_dir_all(&staging_dir);
    fs::create_dir_all(&cache_root)?;
    super::current().clone_tree(payload_dir, &staging_dir)?;
    // Renaming only after the copy fully lands means a crash mid-copy leaves an orphaned
    // `.partial` dir behind instead of a half-written install that could pass as launchable.
    fs::rename(&staging_dir, &final_dir)?;
    prune_stale(&cache_root, &full_name, exe_name);

    Ok(final_exe)
}

fn prune_stale(cache_root: &Path, keep: &str, exe_name: &OsStr) {
    let Ok(entries) = fs::read_dir(cache_root) else {
        return;
    };
    for entry in entries.flatten() {
        if entry.file_name().to_str() == Some(keep) || !entry.path().is_dir() {
            continue;
        }
        let dir = entry.path();
        let exe = dir.join(exe_name);
        // A failed unlink while the file still exists means the copy is still running (Windows
        // locks mapped exe images); skip the whole dir rather than gutting a live install.
        if fs::remove_file(&exe).is_err() && exe.exists() {
            continue;
        }
        let _ = fs::remove_dir_all(&dir);
    }
}

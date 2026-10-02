//! Retires the payload cache that earlier cdm versions launched Claude from. Package-store
//! binaries are activated in place now, so nothing here makes copies any more.

use std::fs;

const CACHE_DIR_NAME: &str = "msix-app";

/// Best-effort and infallible: a dir whose exe is still running is left for a later launch.
pub(super) fn remove_copies() {
    let Ok(local_app_data) = super::env_dir(super::win32::LOCAL_APP_DATA) else {
        return;
    };
    let cache_root = local_app_data
        .join(super::MANAGER_DIR_NAME)
        .join(CACHE_DIR_NAME);
    let Ok(entries) = fs::read_dir(&cache_root) else {
        return;
    };
    for entry in entries.flatten() {
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        let exe = dir.join(super::win32::EXE_NAME);
        // A failed unlink while the file still exists means the copy is still running (Windows
        // locks mapped exe images); skip the whole dir rather than gutting a live install.
        if fs::remove_file(&exe).is_err() && exe.exists() {
            continue;
        }
        let _ = fs::remove_dir_all(&dir);
    }
    // Not remove_dir_all: a root that still holds a live copy must survive.
    let _ = fs::remove_dir(&cache_root);
}

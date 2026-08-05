//! macOS adapter. Verified against Claude Desktop 1.25927.0 on Darwin 25.5.

use super::{Platform, ProfileProcesses};
use crate::core::types::{CdmError, Result};
use std::fs::File;
use std::os::unix::io::{AsRawFd, RawFd};
use std::path::{Path, PathBuf};
use std::process::Command;

pub(super) struct Darwin;

const BUNDLE_NAME: &str = "Claude.app";
/// UNVERIFIED: read `:CFBundleIdentifier` off the installed bundle to confirm.
const BUNDLE_ID: &str = "com.anthropic.claudefordesktop";
const APP_SUPPORT: &str = "Library/Application Support";
const PLIST_BUDDY: &str = "/usr/libexec/PlistBuddy";
const MDFIND: &str = "/usr/bin/mdfind";

impl Platform for Darwin {
    fn find_claude_binary(&self) -> Result<PathBuf> {
        if let Some(path) = super::override_binary()? {
            return Ok(path);
        }
        if let Some(exe) = standard_bundles().iter().find_map(|b| executable_in(b)) {
            return Ok(exe);
        }
        spotlight_bundles()
            .iter()
            .find_map(|b| executable_in(b))
            .ok_or(CdmError::BinaryNotFound)
    }

    fn profiles_root(&self) -> Result<PathBuf> {
        Ok(super::env_dir("HOME")?.join(APP_SUPPORT))
    }

    fn manager_data_dir(&self) -> Result<PathBuf> {
        Ok(self.profiles_root()?.join(super::MANAGER_DIR_NAME))
    }

    fn launch(&self, binary: &Path, data_dir: &Path) -> Result<u32> {
        super::spawn_detached(binary, data_dir)
    }

    fn is_running(&self, data_dir: &Path) -> Result<Option<u32>> {
        // Chromium's `SingletonLock` is never created — Claude never calls
        // requestSingleInstanceLock — so leveldb's LOCK is the only liveness signal, and only
        // its held-ness counts: the file itself outlives a clean shutdown.
        let lock = probe_lock(&super::liveness_lock_path(data_dir));
        if lock.present && !lock.held {
            return Ok(None);
        }
        if let Some(pid) = super::processes_for(data_dir).main.or(lock.holder) {
            return Ok(Some(pid));
        }
        if lock.held {
            return Err(CdmError::Other(format!(
                "{} is locked by a live process that could not be identified",
                data_dir.display()
            )));
        }
        Ok(None)
    }

    fn terminate(&self, pid: u32, data_dir: &Path) -> Result<()> {
        // Measured: exits in ~1s taking every helper with it, Preferences byte-identical.
        signal(pid, libc::SIGTERM);
        if !super::wait_until(super::TERM_GRACE, || !alive(pid)) {
            signal(pid, libc::SIGKILL);
            super::wait_until(super::KILL_GRACE, || !alive(pid));
        }

        let ProfileProcesses { all, .. } = super::processes_for(data_dir);
        for orphan in all {
            signal(orphan, libc::SIGKILL);
        }

        let stopped = super::wait_until(super::LOCK_RELEASE_GRACE, || {
            matches!(self.is_running(data_dir), Ok(None))
        });
        if stopped {
            Ok(())
        } else {
            Err(CdmError::Other(format!(
                "{} is still in use after SIGKILL",
                data_dir.display()
            )))
        }
    }

    fn trash(&self, path: &Path) -> Result<()> {
        super::trash_path(path)
    }
}

fn standard_bundles() -> Vec<PathBuf> {
    let mut bundles = vec![PathBuf::from("/Applications").join(BUNDLE_NAME)];
    if let Ok(home) = super::env_dir("HOME") {
        bundles.push(home.join("Applications").join(BUNDLE_NAME));
    }
    bundles
}

fn spotlight_bundles() -> Vec<PathBuf> {
    let Ok(output) = Command::new(MDFIND)
        .arg(format!("kMDItemCFBundleIdentifier == '{BUNDLE_ID}'"))
        .output()
    else {
        return Vec::new();
    };
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| !line.is_empty())
        .map(PathBuf::from)
        .collect()
}

fn executable_in(bundle: &Path) -> Option<PathBuf> {
    let macos = bundle.join("Contents").join("MacOS");
    if let Some(name) = bundle_executable(bundle) {
        let candidate = macos.join(name);
        if super::is_executable_file(&candidate) {
            return Some(candidate);
        }
    }
    sole_entry(&macos).filter(|path| super::is_executable_file(path))
}

fn bundle_executable(bundle: &Path) -> Option<String> {
    let output = Command::new(PLIST_BUDDY)
        .arg("-c")
        .arg("Print :CFBundleExecutable")
        .arg(bundle.join("Contents").join("Info.plist"))
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let name = String::from_utf8(output.stdout).ok()?.trim().to_string();
    (!name.is_empty() && !name.contains('/')).then_some(name)
}

fn sole_entry(dir: &Path) -> Option<PathBuf> {
    let mut entries = std::fs::read_dir(dir).ok()?.flatten();
    let only = entries.next()?;
    entries.next().is_none().then(|| only.path())
}

struct LockState {
    present: bool,
    held: bool,
    holder: Option<u32>,
}

fn probe_lock(path: &Path) -> LockState {
    let Ok(file) = File::open(path) else {
        return LockState { present: false, held: false, holder: None };
    };
    let fd = file.as_raw_fd();
    // leveldb takes an fcntl record lock; fcntl and flock are separate lock spaces on some
    // kernels, so ask both and treat either answer as held.
    let holder = record_lock_holder(fd);
    LockState { present: true, held: holder.is_some() || flock_blocked(fd), holder }
}

fn flock_blocked(fd: RawFd) -> bool {
    unsafe {
        if libc::flock(fd, libc::LOCK_EX | libc::LOCK_NB) == 0 {
            libc::flock(fd, libc::LOCK_UN);
            return false;
        }
    }
    std::io::Error::last_os_error().raw_os_error() == Some(libc::EWOULDBLOCK)
}

fn record_lock_holder(fd: RawFd) -> Option<u32> {
    let mut probe: libc::flock = unsafe { std::mem::zeroed() };
    probe.l_type = libc::F_WRLCK as libc::c_short;
    probe.l_whence = libc::SEEK_SET as libc::c_short;
    let queried = unsafe { libc::fcntl(fd, libc::F_GETLK, &mut probe as *mut libc::flock) };
    if queried != 0 {
        return None;
    }
    (probe.l_type != libc::F_UNLCK as libc::c_short && probe.l_pid > 0).then_some(probe.l_pid as u32)
}

fn signal(pid: u32, sig: libc::c_int) {
    // kill(0, …) would signal cdm's own process group.
    if pid == 0 {
        return;
    }
    unsafe { libc::kill(pid as libc::pid_t, sig) };
}

fn alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    if unsafe { libc::kill(pid as libc::pid_t, 0) } == 0 {
        return true;
    }
    std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

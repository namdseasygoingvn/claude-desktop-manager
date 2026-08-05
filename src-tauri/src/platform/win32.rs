//! Windows adapter. **UNVERIFIED**: no Windows hardware was available, so every install path,
//! executable name and lock behaviour below is a projection. See `plan/01-platform-adapter.md`
//! for the checks that close each one.

use super::{Platform, ProfileProcesses};
use crate::core::types::{CdmError, Result};
use std::fs::File;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use sysinfo::{Pid, System};

pub(super) struct Win32;

pub(super) const DETACHED_PROCESS: u32 = 0x0000_0008;
pub(super) const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
/// Ignored when combined with DETACHED_PROCESS, so it belongs on console helpers only.
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

const TASKKILL: &str = "taskkill";
/// UNVERIFIED: the Squirrel stub at the install root is the version-stable target.
const EXE_NAME: &str = "claude.exe";
const INSTALL_ROOTS: [(&str, &str); 3] = [
    ("LOCALAPPDATA", "AnthropicClaude"),
    ("LOCALAPPDATA", r"Programs\AnthropicClaude"),
    ("PROGRAMFILES", "AnthropicClaude"),
];

const ERROR_SHARING_VIOLATION: i32 = 32;
const ERROR_LOCK_VIOLATION: i32 = 33;

impl Platform for Win32 {
    fn find_claude_binary(&self) -> Result<PathBuf> {
        if let Some(path) = super::override_binary()? {
            return Ok(path);
        }
        // UNVERIFIED: no registry probe of Uninstall\…\InstallLocation yet; a non-standard
        // install is reachable through CDM_CLAUDE_BINARY.
        INSTALL_ROOTS
            .iter()
            .filter_map(|(var, sub)| super::env_dir(var).ok().map(|root| root.join(sub).join(EXE_NAME)))
            .find(|candidate| super::is_executable_file(candidate))
            .ok_or(CdmError::BinaryNotFound)
    }

    fn profiles_root(&self) -> Result<PathBuf> {
        super::env_dir("APPDATA")
    }

    fn manager_data_dir(&self) -> Result<PathBuf> {
        Ok(self.profiles_root()?.join(super::MANAGER_DIR_NAME))
    }

    fn launch(&self, binary: &Path, data_dir: &Path) -> Result<u32> {
        // UNVERIFIED: if the stub execs the real binary and exits, this pid is short-lived and
        // `is_running` resolves the survivor by argv instead.
        super::spawn_detached(binary, data_dir)
    }

    fn is_running(&self, data_dir: &Path) -> Result<Option<u32>> {
        // The argv scan leads here because the lock probe cannot name a pid on Windows.
        if let Some(pid) = super::processes_for(data_dir).main {
            return Ok(Some(pid));
        }
        // UNVERIFIED: leveldb's Windows env opens LOCK with no sharing, so a sharing violation
        // means held. Existence alone never does — the file survives a clean shutdown.
        if lock_held(&super::liveness_lock_path(data_dir)) {
            return Err(CdmError::Other(format!(
                "{} is locked by a live process that could not be identified (sysinfo may need \
                 elevation to read another process's command line)",
                data_dir.display()
            )));
        }
        Ok(None)
    }

    fn terminate(&self, pid: u32, data_dir: &Path) -> Result<()> {
        // Bare taskkill posts WM_CLOSE — the honest SIGTERM analogue. /F is TerminateProcess.
        taskkill(pid, false);
        if !super::wait_until(super::TERM_GRACE, || !alive(pid)) {
            taskkill(pid, true);
            super::wait_until(super::KILL_GRACE, || !alive(pid));
        }

        let ProfileProcesses { all, .. } = super::processes_for(data_dir);
        for orphan in all {
            taskkill(orphan, true);
        }

        // Handle release lags process exit here, and a rename fails while any handle is open.
        let stopped = super::wait_until(super::LOCK_RELEASE_GRACE, || {
            matches!(self.is_running(data_dir), Ok(None))
        });
        if stopped {
            Ok(())
        } else {
            Err(CdmError::Other(format!(
                "{} is still in use after taskkill /F /T",
                data_dir.display()
            )))
        }
    }

    fn trash(&self, path: &Path) -> Result<()> {
        super::trash_path(path)
    }
}

fn lock_held(path: &Path) -> bool {
    match File::open(path) {
        Ok(_) => false,
        Err(e) => matches!(
            e.raw_os_error(),
            Some(ERROR_SHARING_VIOLATION) | Some(ERROR_LOCK_VIOLATION)
        ),
    }
}

fn taskkill(pid: u32, force: bool) {
    if pid == 0 {
        return;
    }
    let pid = pid.to_string();
    let mut cmd = Command::new(TASKKILL);
    if force {
        cmd.args(["/F", "/T"]);
    }
    // Always addressed by pid, never by image name: a name would hit every profile.
    cmd.args(["/PID", pid.as_str()])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW);
    let _ = cmd.status();
}

fn alive(pid: u32) -> bool {
    pid != 0 && System::new_all().process(Pid::from_u32(pid)).is_some()
}

//! Windows adapter. The install layout `find_claude_binary` probes — Squirrel's versioned
//! directories and the MSIX app-execution alias — comes from public reports of both shipping
//! channels. **UNVERIFIED**: no Windows hardware was available, so launch, process and lock
//! behaviour below is still a projection. See `plan/01-platform-adapter.md` for the checks that
//! close each one.

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
const REG: &str = "reg";
const EXE_NAME: &str = "claude.exe";
const LOCAL_APP_DATA: &str = "LOCALAPPDATA";
const INSTALL_ROOTS: [(&str, &str); 3] = [
    (LOCAL_APP_DATA, "AnthropicClaude"),
    (LOCAL_APP_DATA, r"Programs\AnthropicClaude"),
    ("PROGRAMFILES", "AnthropicClaude"),
];
const UNINSTALL_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\Claude";
/// The MSIX channel installs no enumerable directory and writes no uninstall key; this alias is
/// the only thing it leaves behind that can be launched.
const MSIX_ALIAS_DIR: &str = r"Microsoft\WindowsApps";
const VERSION_DIR_PREFIX: &str = "app-";

const ERROR_SHARING_VIOLATION: i32 = 32;
const ERROR_LOCK_VIOLATION: i32 = 33;

impl Platform for Win32 {
    fn find_claude_binary(&self) -> Result<PathBuf> {
        if let Some(path) = super::override_binary()? {
            return Ok(path);
        }
        install_roots()
            .into_iter()
            .flat_map(|root| [newest_versioned_exe(&root), Some(root.join(EXE_NAME))])
            .flatten()
            .chain(msix_alias())
            .find(|candidate| super::is_executable_file(candidate))
            .ok_or(CdmError::BinaryNotFound)
    }

    fn binary_picker(&self) -> (&'static str, &'static [&'static str], Option<PathBuf>) {
        ("Application", &["exe"], super::env_dir(LOCAL_APP_DATA).ok())
    }

    fn resolve_picked_binary(&self, picked: &Path) -> Result<PathBuf> {
        let refused = || CdmError::NotClaude(picked.display().to_string());
        let name = picked.file_name().unwrap_or_default().to_string_lossy().to_lowercase();
        if !name.contains("claude") || !super::is_executable_file(picked) {
            return Err(refused());
        }
        // Picking the install root reaches the stub, not the app; take the versioned exe beside it.
        if let Some(exe) = picked.parent().and_then(newest_versioned_exe) {
            return Ok(exe);
        }
        Ok(picked.to_path_buf())
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

    fn clone_tree(&self, src: &Path, dst: &Path) -> Result<()> {
        // NTFS has no copy-on-write clone, so this costs real disk. The store still earns its
        // keep by sparing every profile after the first the download.
        super::copy_tree(src, dst)
    }
}

/// Squirrel installs are reachable by convention or by the uninstall key it writes; both are
/// probed because a relocated install leaves only the key, and the key is absent on MSIX.
fn install_roots() -> Vec<PathBuf> {
    INSTALL_ROOTS
        .iter()
        .filter_map(|(var, sub)| super::env_dir(var).ok().map(|root| root.join(sub)))
        .chain(registry_install_location())
        .collect()
}

fn registry_install_location() -> Option<PathBuf> {
    let output = Command::new(REG)
        .args(["query", UNINSTALL_KEY, "/v", "InstallLocation"])
        .stdin(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let line = text
        .lines()
        .find(|line| line.trim_start().starts_with("InstallLocation"))?;
    let (_, path) = line.split_once("REG_SZ")?;
    let path = path.trim();
    (!path.is_empty()).then(|| PathBuf::from(path))
}

/// Squirrel keeps the real binary under `app-<version>`; the root exe is only a launcher stub and
/// has been reported crashing when started bare.
fn newest_versioned_exe(root: &Path) -> Option<PathBuf> {
    std::fs::read_dir(root)
        .ok()?
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name();
            let version = name.to_str()?.strip_prefix(VERSION_DIR_PREFIX)?.to_string();
            Some((version_key(&version), entry.path().join(EXE_NAME)))
        })
        .filter(|(_, exe)| super::is_executable_file(exe))
        .max_by(|a, b| a.0.cmp(&b.0))
        .map(|(_, exe)| exe)
}

fn version_key(version: &str) -> Vec<u64> {
    version.split('.').map(|part| part.parse().unwrap_or(0)).collect()
}

fn msix_alias() -> Option<PathBuf> {
    super::env_dir(LOCAL_APP_DATA)
        .ok()
        .map(|local| local.join(MSIX_ALIAS_DIR).join(EXE_NAME))
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

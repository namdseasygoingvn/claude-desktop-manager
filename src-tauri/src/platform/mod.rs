//! The complete surface of OS difference. Core never branches on platform.

use crate::core::types::{CdmError, Result};
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};
use sysinfo::System;

#[cfg(target_os = "macos")]
mod darwin;
#[cfg(target_os = "windows")]
mod win32;

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
compile_error!("cdm supports macOS and Windows only");

#[cfg(target_os = "macos")]
use darwin::Darwin as Current;
#[cfg(target_os = "windows")]
use win32::Win32 as Current;

pub const BINARY_OVERRIDE_ENV: &str = "CDM_CLAUDE_BINARY";
pub const USER_DATA_DIR_ENV: &str = "CLAUDE_USER_DATA_DIR";
pub const USER_DATA_DIR_ARG: &str = "--user-data-dir=";
pub const MANAGER_DIR_NAME: &str = "ClaudeDesktopManager";

const HELPER_TYPE_ARG: &str = "--type=";
const LIVENESS_LOCK: [&str; 3] = ["Local Storage", "leveldb", "LOCK"];
const TERM_GRACE: Duration = Duration::from_secs(5);
const KILL_GRACE: Duration = Duration::from_secs(3);
const LOCK_RELEASE_GRACE: Duration = Duration::from_secs(3);
const POLL_INTERVAL: Duration = Duration::from_millis(200);

pub trait Platform: Send + Sync {
    /// `CDM_CLAUDE_BINARY` first, then the platform default. Never cached: the app self-updates.
    fn find_claude_binary(&self) -> Result<PathBuf>;
    /// Directory that holds the `Claude-*` profile folders.
    fn profiles_root(&self) -> Result<PathBuf>;
    /// Directory that holds `registry.json`.
    fn manager_data_dir(&self) -> Result<PathBuf>;
    /// Spawn detached and yield the main pid.
    fn launch(&self, binary: &Path, data_dir: &Path) -> Result<u32>;
    /// `Ok(Some(pid))` running, `Ok(None)` not running, `Err` undecidable — never guess `None`.
    fn is_running(&self, data_dir: &Path) -> Result<Option<u32>>;
    /// Graceful stop escalating to a hard kill, then sweep helpers still holding `data_dir`.
    fn terminate(&self, pid: u32, data_dir: &Path) -> Result<()>;
    /// Move to Trash / Recycle Bin. `Err` when the platform has none available.
    fn trash(&self, path: &Path) -> Result<()>;
}

static CURRENT: Current = Current;

pub fn current() -> &'static dyn Platform {
    &CURRENT
}

pub(crate) fn liveness_lock_path(data_dir: &Path) -> PathBuf {
    LIVENESS_LOCK
        .iter()
        .fold(data_dir.to_path_buf(), |path, part| path.join(part))
}

fn io_err(context: &str, e: std::io::Error) -> CdmError {
    CdmError::Io(format!("{context}: {e}"))
}

fn trash_path(path: &Path) -> Result<()> {
    trash::delete(path).map_err(|e| CdmError::Io(format!("move to trash failed: {e}")))
}

fn override_binary() -> Result<Option<PathBuf>> {
    let Some(raw) = std::env::var_os(BINARY_OVERRIDE_ENV) else {
        return Ok(None);
    };
    if raw.is_empty() {
        return Ok(None);
    }
    let path = PathBuf::from(raw);
    if !is_executable_file(&path) {
        return Err(CdmError::Other(format!(
            "{BINARY_OVERRIDE_ENV} is not an executable file: {}",
            path.display()
        )));
    }
    Ok(Some(path))
}

fn is_executable_file(path: &Path) -> bool {
    fs::metadata(path).map(|md| is_executable(&md)).unwrap_or(false)
}

#[cfg(unix)]
fn is_executable(md: &fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    md.is_file() && md.permissions().mode() & 0o111 != 0
}

#[cfg(windows)]
fn is_executable(md: &fs::Metadata) -> bool {
    md.is_file()
}

fn canonical(path: &Path) -> PathBuf {
    strip_verbatim(path.canonicalize().unwrap_or_else(|_| path.to_path_buf()))
}

#[cfg(unix)]
fn strip_verbatim(path: PathBuf) -> PathBuf {
    path
}

#[cfg(windows)]
fn strip_verbatim(path: PathBuf) -> PathBuf {
    // canonicalize() yields the `\\?\` form, which Chromium and Squirrel stubs do not expect.
    let plain = path
        .to_str()
        .and_then(|s| s.strip_prefix(r"\\?\"))
        .filter(|rest| rest.as_bytes().get(1) == Some(&b':'))
        .map(PathBuf::from);
    plain.unwrap_or(path)
}

fn spawn_detached(binary: &Path, data_dir: &Path) -> Result<u32> {
    let dir = canonical(data_dir);
    let mut flag = OsString::from(USER_DATA_DIR_ARG);
    flag.push(&dir);

    let mut cmd = Command::new(binary);
    // The env var is read before anything else and short-circuits the asar's `custom-3p`
    // branch, the one guarded `app.setPath('userData')` that would defeat the flag.
    cmd.arg(flag)
        .env(USER_DATA_DIR_ENV, &dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    detach(&mut cmd);

    let child = cmd
        .spawn()
        .map_err(|e| io_err(&format!("spawn {}", binary.display()), e))?;
    let pid = child.id();
    reap_in_background(child);
    Ok(pid)
}

#[cfg(unix)]
fn detach(cmd: &mut Command) {
    use std::os::unix::process::CommandExt;
    cmd.process_group(0);
}

#[cfg(windows)]
fn detach(cmd: &mut Command) {
    use std::os::windows::process::CommandExt;
    cmd.creation_flags(win32::DETACHED_PROCESS | win32::CREATE_NEW_PROCESS_GROUP);
}

fn reap_in_background(mut child: Child) {
    // An un-waited child lingers as a zombie for cdm's whole life and corrupts pid liveness.
    std::thread::spawn(move || {
        let _ = child.wait();
    });
}

pub(crate) struct ProfileProcesses {
    pub main: Option<u32>,
    pub all: Vec<u32>,
}

/// Every process whose argv carries this exact data dir, and the lowest-pid non-helper among them.
pub(crate) fn processes_for(data_dir: &Path) -> ProfileProcesses {
    let targets = [data_dir.to_path_buf(), canonical(data_dir)];
    let system = System::new_all();
    let mut main = Vec::new();
    let mut all = Vec::new();

    for (pid, process) in system.processes() {
        let argv = process.cmd();
        if !uses_data_dir(argv, &targets) {
            continue;
        }
        let pid = pid.as_u32();
        all.push(pid);
        if !argv.iter().any(|arg| is_helper_arg(arg)) {
            main.push(pid);
        }
    }

    all.sort_unstable();
    main.sort_unstable();
    ProfileProcesses { main: main.first().copied(), all }
}

fn is_helper_arg(arg: &OsString) -> bool {
    arg.to_string_lossy().starts_with(HELPER_TYPE_ARG)
}

fn uses_data_dir(argv: &[OsString], targets: &[PathBuf]) -> bool {
    argv.iter().any(|arg| {
        let text = arg.to_string_lossy();
        let Some(value) = text.strip_prefix(USER_DATA_DIR_ARG) else {
            return false;
        };
        // Exact, never substring: `…/Claude-Work` is a prefix of `…/Claude-Work-2`, and `-2`
        // is cdm's own collision suffix.
        let value = Path::new(value);
        let resolved = value.canonicalize().ok();
        targets
            .iter()
            .any(|t| t.as_path() == value || Some(t.as_path()) == resolved.as_deref())
    })
}

fn wait_until(timeout: Duration, mut done: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        if done() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(POLL_INTERVAL);
    }
}

fn env_dir(var: &str) -> Result<PathBuf> {
    std::env::var_os(var)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| CdmError::Other(format!("{var} is not set")))
}

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
    /// Duplicate a directory tree, copy-on-write where the filesystem offers it. `dst` must not
    /// exist. The result is an independent tree either way — only the disk blocks are shared.
    fn clone_tree(&self, src: &Path, dst: &Path) -> Result<()>;
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

/// Two different questions, deliberately answered by two different rules.
///
/// `main` is the Electron app, and only an exact `--user-data-dir=` may name it — a claude-code
/// child mistaken for the app would make `is_running` report the wrong pid.
///
/// `all` is everything the profile is responsible for, matched on the data dir appearing
/// *anywhere*: crashpad carries it in `--database=`, local-agent-mode children in `--plugin-dir`
/// and in their own exec path. A flag-only sweep saw none of them, so they outlived every quit
/// and went on burning a core each as orphans.
pub(crate) fn processes_for(data_dir: &Path) -> ProfileProcesses {
    let targets = [data_dir.to_path_buf(), canonical(data_dir)];
    let own = std::process::id();
    let system = System::new_all();
    let mut main = Vec::new();
    let mut all = Vec::new();

    for (pid, process) in system.processes() {
        let pid = pid.as_u32();
        if pid == own {
            continue;
        }
        let argv = process.cmd();
        if !mentions_data_dir(argv, process.exe(), &targets) {
            continue;
        }
        all.push(pid);
        if uses_data_dir(argv, &targets) && !argv.iter().any(is_helper_arg) {
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

fn mentions_data_dir(argv: &[OsString], exe: Option<&Path>, targets: &[PathBuf]) -> bool {
    let exe = exe.map(|path| path.to_string_lossy());
    argv.iter()
        .map(|arg| arg.to_string_lossy())
        .chain(exe)
        .any(|text| targets.iter().any(|target| holds_path(&text, target)))
}

/// Substring, but only where the hit is a whole path component run. The same collision the
/// exact match guards against applies here: `…/Claude-Work` must not match `…/Claude-Work-2`,
/// and an absolute path must not match one merely suffixed by it.
fn holds_path(text: &str, target: &Path) -> bool {
    let needle = target.to_string_lossy();
    if needle.is_empty() {
        return false;
    }
    text.match_indices(needle.as_ref()).any(|(at, hit)| {
        let opens = at == 0
            || matches!(text.as_bytes()[at - 1], b'=' | b':' | b',' | b' ' | b'"' | b'\'');
        let closes = match text[at + hit.len()..].chars().next() {
            None => true,
            Some(next) => next == std::path::MAIN_SEPARATOR,
        };
        opens && closes
    })
}

/// Block-for-block duplication, for filesystems with no clone call of their own.
fn copy_tree(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst).map_err(|e| io_err(&format!("create {}", dst.display()), e))?;
    let entries = fs::read_dir(src).map_err(|e| io_err(&format!("read {}", src.display()), e))?;
    for entry in entries {
        let entry = entry.map_err(|e| io_err(&format!("read {}", src.display()), e))?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        let kind = entry
            .file_type()
            .map_err(|e| io_err(&format!("stat {}", from.display()), e))?;
        if kind.is_dir() {
            copy_tree(&from, &to)?;
        } else if kind.is_file() {
            fs::copy(&from, &to).map_err(|e| io_err(&format!("copy to {}", to.display()), e))?;
        }
    }
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    const DIR: &str = "/Users/x/Library/Application Support/Claude-Work";

    fn argv(args: &[&str]) -> Vec<OsString> {
        args.iter().map(OsString::from).collect()
    }

    fn mentions(args: &[&str]) -> bool {
        mentions_data_dir(&argv(args), None, &[PathBuf::from(DIR)])
    }

    /// The three shapes measured on a machine whose profiles had leaked: none carries
    /// `--user-data-dir`, and all three outlived every quit before the sweep learned to see them.
    #[test]
    fn the_children_that_leaked_are_all_matched() {
        assert!(mentions(&["--database=/Users/x/Library/Application Support/Claude-Work/Crashpad"]));
        assert!(mentions(&[
            "--plugin-dir",
            "/Users/x/Library/Application Support/Claude-Work/local-agent-mode-sessions/a/b"
        ]));
        assert!(mentions_data_dir(
            &argv(&["claude", "--verbose"]),
            Some(Path::new(
                "/Users/x/Library/Application Support/Claude-Work/claude-code/2.1.221/claude"
            )),
            &[PathBuf::from(DIR)],
        ));
    }

    /// The collision the exact match already guarded against, now that matching is a substring:
    /// `-2` is cdm's own suffix, so a sibling profile must never be swept up with this one.
    #[test]
    fn a_sibling_profile_sharing_the_prefix_is_never_matched() {
        assert!(!mentions(&[
            "--user-data-dir=/Users/x/Library/Application Support/Claude-Work-2"
        ]));
        assert!(!mentions(&[
            "--database=/Users/x/Library/Application Support/Claude-Work-2/Crashpad"
        ]));
    }

    #[test]
    fn a_path_merely_suffixed_by_the_profile_is_never_matched() {
        assert!(!mentions(&[
            "--database=/elsewhere/Users/x/Library/Application Support/Claude-Work/Crashpad"
        ]));
    }

    #[test]
    fn an_unrelated_process_is_never_matched() {
        assert!(!mentions(&["/bin/zsh", "-l"]));
        assert!(!mentions(&[]));
    }

    /// Broad matching feeds the sweep only. The app itself stays pinned to the exact flag, or a
    /// claude-code child would be reported as the running app.
    #[test]
    fn only_the_exact_flag_names_the_app_itself() {
        let targets = [PathBuf::from(DIR)];
        let child = argv(&[
            "/Users/x/Library/Application Support/Claude-Work/claude-code/2.1.221/claude",
        ]);
        assert!(mentions_data_dir(&child, None, &targets));
        assert!(!uses_data_dir(&child, &targets));
        assert!(uses_data_dir(&argv(&[&format!("--user-data-dir={DIR}")]), &targets));
    }
}

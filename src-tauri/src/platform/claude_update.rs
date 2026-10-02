//! Claude package updates live in cdm because profiles run identity-less copies of the payload,
//! so Claude's own updater disables itself there. cdm checks the MSIX feed and installs for the user.

use super::{claude_feed, msix, package_version, ProcessTable};
use crate::core::types::{CdmError, Result};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use sysinfo::Pid;

use super::win32::{CREATE_NO_WINDOW, LOCAL_APP_DATA};
use super::ClaudePackageUpdate;

const CURL: &str = "curl.exe";
const POWERSHELL: &str = "powershell";
const APPDATA: &str = "APPDATA";
const CHECK_TIMEOUT_SECS: &str = "20";
const DOWNLOAD_TIMEOUT_SECS: &str = "900";
const OVERLAY_PACKAGES_DIR: &str = "Packages";
const OVERLAY_ROAMING_DIR: &[&str] = &["LocalCache", "Roaming"];

struct Latest {
    installed: String,
    release: claude_feed::LatestRelease,
}

impl Latest {
    fn available(&self) -> bool {
        package_version::is_newer(&self.release.version, &self.installed)
    }
}

pub(super) fn check() -> Result<Option<ClaudePackageUpdate>> {
    Ok(latest()?.map(|latest| ClaudePackageUpdate {
        available: latest.available(),
        installed: latest.installed,
        latest: latest.release.version,
    }))
}

pub(super) fn install() -> Result<()> {
    let Some(latest) = latest()?.filter(Latest::available) else {
        return Ok(());
    };
    let msix_file =
        std::env::temp_dir().join(format!("cdm-claude-{}.msix", latest.release.version));
    let outcome = download(&latest.release.url, &msix_file).and_then(|()| add_package(&msix_file));
    let _ = std::fs::remove_file(&msix_file);
    outcome
}

pub(super) fn runs_outdated(table: &ProcessTable, data_dir: &Path) -> bool {
    let Some(pid) = super::processes_for(table, data_dir).main else {
        return false;
    };
    let Some(process) = table.0.process(Pid::from_u32(pid)) else {
        return false;
    };
    let Some(exe) = process.exe() else {
        return false;
    };
    match (
        package_version::running_package(exe),
        msix::newest_claude_package(),
    ) {
        (Some(running), Some(newest)) => running != newest,
        _ => false,
    }
}

pub(super) fn virtualized_copy_of(path: &Path) -> Option<PathBuf> {
    let relative = path.strip_prefix(super::env_dir(APPDATA).ok()?).ok()?;
    let family = package_version::family_of(&msix::newest_claude_package()?)?;
    let mut overlay = super::env_dir(LOCAL_APP_DATA)
        .ok()?
        .join(OVERLAY_PACKAGES_DIR)
        .join(family);
    overlay.extend(OVERLAY_ROAMING_DIR);
    Some(overlay.join(relative))
}

fn latest() -> Result<Option<Latest>> {
    let Some(newest) = msix::newest_claude_package() else {
        return Ok(None);
    };
    let (Some(installed), Some(arch)) = (
        package_version::version_of(&newest),
        package_version::arch_of(&newest),
    ) else {
        return Ok(None);
    };
    let body = run_checked(Command::new(CURL).args([
        "-fsS",
        "--max-time",
        CHECK_TIMEOUT_SECS,
        &claude_feed::feed_url(arch),
    ]))?;
    let release = claude_feed::parse(&String::from_utf8_lossy(&body.stdout))
        .ok_or_else(|| CdmError::Other("unreadable update feed".into()))?;
    Ok(Some(Latest {
        installed: installed.to_string(),
        release,
    }))
}

fn download(url: &str, destination: &Path) -> Result<()> {
    run_checked(
        Command::new(CURL)
            .args(["-fsSL", "--max-time", DOWNLOAD_TIMEOUT_SECS, "-o"])
            .arg(destination)
            .arg(url),
    )
    .map(drop)
}

fn add_package(msix_file: &Path) -> Result<()> {
    let literal = msix_file.to_string_lossy().replace('\'', "''");
    run_checked(Command::new(POWERSHELL).args([
        "-NoProfile",
        "-NonInteractive",
        "-Command",
        &format!("Add-AppxPackage -Path '{literal}' -ForceApplicationShutdown"),
    ]))
    .map(drop)
}

fn run_checked(command: &mut Command) -> Result<Output> {
    let output = command
        .stdin(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .output()?;
    if output.status.success() {
        return Ok(output);
    }
    Err(CdmError::Other(last_line(&output.stderr)))
}

fn last_line(stderr: &[u8]) -> String {
    String::from_utf8_lossy(stderr)
        .lines()
        .rev()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("command failed")
        .to_string()
}

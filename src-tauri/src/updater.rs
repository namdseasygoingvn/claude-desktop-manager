use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tauri_plugin_updater::{Update, UpdaterExt};

use crate::commands::{CmdResult, CommandError};

const POLL_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);

/// Chunks land far faster than a webview can paint; anything sooner than this folds into the
/// next tick.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);

const PROGRESS_EVENT: &str = "cdm://update-progress";

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase", tag = "status")]
pub enum UpdateOutcome {
    UpToDate { version: String },
    Available { version: String },
}

/// Byte counts only. Speed and time-remaining are smoothed in the webview, where the averaging
/// window is a presentation choice.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase", tag = "step")]
enum Progress {
    Downloading { downloaded: u64, total: Option<u64> },
    Unpacking,
}

/// Only a user-initiated install reports; the background poll stays silent, or a progress bar
/// would appear in a window nobody asked to update.
fn emit_progress(app: &AppHandle, report: bool, progress: Progress) {
    if report {
        let _ = app.emit(PROGRESS_EVENT, progress);
    }
}

async fn pending(app: &AppHandle) -> Result<Option<Update>, String> {
    let updater = app.updater().map_err(|e| e.to_string())?;
    updater.check().await.map_err(|e| e.to_string())
}

/// On macOS/Linux, installing replaces the bundle on disk but leaves this process running the
/// old code, so the update lands on the next launch — either the user's own, or `restart_app`.
/// On Windows, the plugin hands off to the NSIS/MSI installer and exits this process instead, so
/// this must only run for a user-initiated install there (see `spawn_background_check`).
async fn install_latest(app: &AppHandle, report: bool) -> Result<Option<String>, String> {
    let Some(update) = pending(app).await? else {
        return Ok(None);
    };
    let version = update.version.clone();
    let mut downloaded = 0u64;
    let mut next_emit = Instant::now();
    update
        .download_and_install(
            |len, total| {
                downloaded += len as u64;
                let now = Instant::now();
                // The final chunk always reports, so a bar that was given a total reaches it.
                if now < next_emit && Some(downloaded) != total {
                    return;
                }
                next_emit = now + PROGRESS_INTERVAL;
                emit_progress(app, report, Progress::Downloading { downloaded, total });
            },
            || emit_progress(app, report, Progress::Unpacking),
        )
        .await
        .map_err(|e| e.to_string())?;
    Ok(Some(version))
}

fn failed(detail: String) -> CommandError {
    CommandError { kind: "updateFailed", detail: Some(detail) }
}

/// On Windows this is a no-op: `install_latest` there hands off to the installer and exits the
/// process, so a silent pass would restart the app under the user. The webview's hourly poll
/// (`check_for_updates`) surfaces availability instead, leaving the install to the Update button.
pub fn spawn_background_check(app: &AppHandle) {
    #[cfg(target_os = "windows")]
    let _ = app;
    #[cfg(not(target_os = "windows"))]
    {
        let app = app.clone();
        std::thread::spawn(move || loop {
            match tauri::async_runtime::block_on(install_latest(&app, false)) {
                Ok(Some(version)) => log::info!("updated to {version}; applies on next launch"),
                Ok(None) => {}
                Err(detail) => log::warn!("update check failed: {detail}"),
            }
            std::thread::sleep(POLL_INTERVAL);
        });
    }
}

#[tauri::command]
pub async fn check_for_updates(app: AppHandle) -> CmdResult<UpdateOutcome> {
    match pending(&app).await.map_err(failed)? {
        Some(update) => Ok(UpdateOutcome::Available { version: update.version }),
        None => Ok(UpdateOutcome::UpToDate { version: app.package_info().version.to_string() }),
    }
}

#[tauri::command]
pub async fn install_update(app: AppHandle) -> CmdResult<String> {
    install_latest(&app, true)
        .await
        .map_err(failed)?
        .ok_or_else(|| failed("no update is available".into()))
}

/// Profiles are spawned detached, so they outlive this. The single-instance handle must go first:
/// on Windows the relaunch is a fresh process, and it would hand off to this dying one and exit.
/// On macOS a raw exec of the just-installed bundle fails AMFI validation because LaunchServices
/// has not assessed the new content yet, so the relaunch there goes through `open(1)` instead.
#[tauri::command]
pub fn restart_app(app: AppHandle) -> CmdResult<()> {
    #[cfg(target_os = "macos")]
    {
        if let Some(bundle) = macos_bundle_path() {
            std::process::Command::new("/usr/bin/open")
                .arg("-n")
                .arg(bundle)
                .spawn()
                .map_err(|e| failed(e.to_string()))?;
            tauri_plugin_single_instance::destroy(&app);
            app.exit(0);
            return Ok(());
        }
    }
    tauri_plugin_single_instance::destroy(&app);
    app.restart();
}

#[cfg(target_os = "macos")]
fn macos_bundle_path() -> Option<std::path::PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let bundle = exe.ancestors().nth(3)?;
    (bundle.extension()? == "app").then(|| bundle.to_path_buf())
}

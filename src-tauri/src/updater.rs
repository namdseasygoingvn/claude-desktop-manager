use std::time::Duration;

use serde::Serialize;
use tauri::AppHandle;
use tauri_plugin_updater::{Update, UpdaterExt};

use crate::commands::{CmdResult, CommandError};

const POLL_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase", tag = "status")]
pub enum UpdateOutcome {
    UpToDate { version: String },
    Available { version: String },
}

async fn pending(app: &AppHandle) -> Result<Option<Update>, String> {
    let updater = app.updater().map_err(|e| e.to_string())?;
    updater.check().await.map_err(|e| e.to_string())
}

/// Installing replaces the bundle on disk but leaves this process running the old code, so the
/// update lands on the next launch — either the user's own, or `restart_app`.
async fn install_latest(app: &AppHandle) -> Result<Option<String>, String> {
    let Some(update) = pending(app).await? else {
        return Ok(None);
    };
    let version = update.version.clone();
    update
        .download_and_install(|_, _| {}, || {})
        .await
        .map_err(|e| e.to_string())?;
    Ok(Some(version))
}

fn failed(detail: String) -> CommandError {
    CommandError { kind: "updateFailed", detail: Some(detail) }
}

pub fn spawn_background_check(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || loop {
        match tauri::async_runtime::block_on(install_latest(&app)) {
            Ok(Some(version)) => log::info!("updated to {version}; applies on next launch"),
            Ok(None) => {}
            Err(detail) => log::warn!("update check failed: {detail}"),
        }
        std::thread::sleep(POLL_INTERVAL);
    });
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
    install_latest(&app)
        .await
        .map_err(failed)?
        .ok_or_else(|| failed("no update is available".into()))
}

/// Profiles are spawned detached, so they outlive this. The single-instance handle must go first:
/// on Windows the relaunch is a fresh process, and it would hand off to this dying one and exit.
#[tauri::command]
pub fn restart_app(app: AppHandle) {
    tauri_plugin_single_instance::destroy(&app);
    app.restart();
}

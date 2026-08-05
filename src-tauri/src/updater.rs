use std::time::Duration;

use serde::Serialize;
use tauri::AppHandle;
use tauri_plugin_updater::UpdaterExt;

use crate::commands::{CmdResult, CommandError};

const POLL_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase", tag = "status")]
pub enum UpdateOutcome {
    UpToDate { version: String },
    Installed { version: String },
}

/// Installing replaces the bundle on disk but leaves this process running the old code, so the
/// update lands on the next launch. Restarting here would kill the profiles cdm has spawned.
async fn install_if_available(app: &AppHandle) -> Result<UpdateOutcome, String> {
    let current = app.package_info().version.to_string();
    let updater = app.updater().map_err(|e| e.to_string())?;

    match updater.check().await.map_err(|e| e.to_string())? {
        None => Ok(UpdateOutcome::UpToDate { version: current }),
        Some(update) => {
            let version = update.version.clone();
            update
                .download_and_install(|_, _| {}, || {})
                .await
                .map_err(|e| e.to_string())?;
            Ok(UpdateOutcome::Installed { version })
        }
    }
}

pub fn spawn_background_check(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || loop {
        match tauri::async_runtime::block_on(install_if_available(&app)) {
            Ok(UpdateOutcome::Installed { version }) => {
                log::info!("updated to {version}; applies on next launch");
            }
            Ok(_) => {}
            Err(detail) => log::warn!("update check failed: {detail}"),
        }
        std::thread::sleep(POLL_INTERVAL);
    });
}

#[tauri::command]
pub async fn check_for_updates(app: AppHandle) -> CmdResult<UpdateOutcome> {
    install_if_available(&app).await.map_err(|detail| CommandError {
        kind: "updateFailed",
        detail: Some(detail),
    })
}

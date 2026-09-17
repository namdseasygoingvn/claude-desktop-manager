//! What happens a moment after the app starts: the Preferences window and the latest profile.
//! Both wait on the login item — with it off, the app starts in the tray alone.

use std::thread;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tauri_plugin_autostart::ManagerExt;

use crate::commands::{self, CommandError};
use crate::core::types::Profile;
use crate::core::{profile, settings};
use crate::tray;

/// Right after login the OS is still bringing services up; launching into that has failed.
const SETTLE_DELAY: Duration = Duration::from_secs(2);
pub const LAUNCH_FAILED_EVENT: &str = "cdm://startup-launch-failed";

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct LaunchFailed {
    profile: Profile,
    error: CommandError,
}

pub fn schedule(app: &AppHandle) {
    let app = app.clone();
    thread::spawn(move || {
        thread::sleep(SETTLE_DELAY);
        run(&app);
    });
}

fn run(app: &AppHandle) {
    if !app.autolaunch().is_enabled().unwrap_or(false) {
        return;
    }
    let stored = settings::load();
    if stored.open_preferences_at_start {
        show_preferences(app);
    }
    if stored.open_latest_profile_at_start {
        launch_latest(app);
    }
}

fn show_preferences(app: &AppHandle) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        let _ = tray::show_preferences(&handle);
    });
}

fn launch_latest(app: &AppHandle) {
    let latest = match profile::latest_used() {
        Ok(Some(profile)) => profile,
        Ok(None) => return,
        Err(err) => {
            log::error!("cannot pick the latest profile: {err}");
            return;
        }
    };
    if let Err(error) = commands::launch_profile(app.clone(), latest.id.clone()) {
        // A dialog in a hidden window is no report at all.
        show_preferences(app);
        let _ = app.emit(LAUNCH_FAILED_EVENT, LaunchFailed { profile: latest, error });
    }
}

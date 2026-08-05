use std::path::PathBuf;

use serde::Serialize;
use tauri::AppHandle;

use crate::core::profile;
use crate::core::registry;
use crate::core::types::{AdoptCandidate, CdmError, Profile, ProfileStatus};
use crate::platform;
use crate::tray;

const CONFIG_FILE: &str = "claude_desktop_config.json";

pub type CmdResult<T> = std::result::Result<T, CommandError>;

/// The IPC error shape. `kind` is a stable token; the frontend owns the copy for each one.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandError {
    pub kind: &'static str,
    pub detail: Option<String>,
}

impl From<CdmError> for CommandError {
    fn from(err: CdmError) -> Self {
        let (kind, detail) = match err {
            CdmError::BinaryNotFound => ("binaryNotFound", None),
            CdmError::NameEmpty => ("nameEmpty", None),
            CdmError::ProfileNotFound(d) => ("profileNotFound", Some(d)),
            CdmError::ProfileRunning(d) => ("profileRunning", Some(d)),
            CdmError::DirExists(d) => ("dirExists", Some(d)),
            CdmError::RegistryCorrupt(d) => ("registryCorrupt", Some(d)),
            CdmError::Io(d) => ("io", Some(d)),
            CdmError::Other(d) => ("other", Some(d)),
        };
        Self { kind, detail }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DoctorReport {
    pub binary: Option<String>,
    pub binary_error: Option<CommandError>,
    pub profiles_root: String,
    pub reconcile: serde_json::Value,
}

#[tauri::command]
pub fn list_profiles() -> CmdResult<Vec<ProfileStatus>> {
    Ok(profile::list()?)
}

#[tauri::command]
pub fn create_profile(app: AppHandle, name: String) -> CmdResult<Profile> {
    let profile = profile::create(&name)?;
    let _ = tray::rebuild(&app);
    Ok(profile)
}

#[tauri::command]
pub fn launch_profile(app: AppHandle, id: String) -> CmdResult<u32> {
    let pid = profile::launch(&id)?;
    let _ = tray::rebuild(&app);
    Ok(pid)
}

#[tauri::command]
pub fn rename_profile(app: AppHandle, id: String, new_name: String) -> CmdResult<Profile> {
    let profile = profile::rename(&id, &new_name)?;
    let _ = tray::rebuild(&app);
    Ok(profile)
}

#[tauri::command]
pub fn delete_profile(app: AppHandle, id: String) -> CmdResult<()> {
    profile::delete(&id)?;
    let _ = tray::rebuild(&app);
    Ok(())
}

#[tauri::command]
pub fn quit_profile(app: AppHandle, id: String) -> CmdResult<()> {
    profile::quit(&id)?;
    let _ = tray::rebuild(&app);
    Ok(())
}

#[tauri::command]
pub fn list_adoptable() -> CmdResult<Vec<AdoptCandidate>> {
    Ok(profile::adoptable()?)
}

#[tauri::command]
pub fn adopt_folder(app: AppHandle, dir_name: String, display_name: String) -> CmdResult<Profile> {
    let profile = profile::adopt(&dir_name, &display_name)?;
    let _ = tray::rebuild(&app);
    Ok(profile)
}

#[tauri::command]
pub fn open_config(id: String) -> CmdResult<()> {
    let path = config_path(&id)?;
    // UNVERIFIED API: `tauri_plugin_opener::open_path(path, with)` — name confirmed in
    // plan/02, exact signature is not.
    tauri_plugin_opener::open_path(path, None::<&str>)
        .map_err(|e| CdmError::Io(e.to_string()).into())
}

#[tauri::command]
pub fn doctor(app: AppHandle) -> CmdResult<DoctorReport> {
    let (binary, binary_error) = match platform::current().find_claude_binary() {
        Ok(path) => (Some(path.display().to_string()), None),
        Err(e) => (None, Some(CommandError::from(e))),
    };

    let mut reg = registry::load()?;
    let reconcile =
        serde_json::to_value(registry::reconcile(&mut reg)?).unwrap_or(serde_json::Value::Null);
    let _ = tray::rebuild(&app);

    Ok(DoctorReport {
        binary,
        binary_error,
        profiles_root: profiles_root()?.display().to_string(),
        reconcile,
    })
}

fn config_path(id: &str) -> CmdResult<PathBuf> {
    let dir = profile::list()?
        .into_iter()
        .find(|s| s.profile.id == id)
        .map(|s| s.profile.dir)
        .ok_or_else(|| CommandError::from(CdmError::ProfileNotFound(id.to_string())))?;
    Ok(profiles_root()?.join(dir).join(CONFIG_FILE))
}

fn profiles_root() -> CmdResult<PathBuf> {
    Ok(platform::current().profiles_root()?)
}

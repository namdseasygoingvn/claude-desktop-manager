use std::path::PathBuf;

use serde::Serialize;
use tauri::AppHandle;
use tauri_plugin_autostart::ManagerExt;

use crate::core::groups;
use crate::core::profile;
use crate::core::registry;
use crate::core::settings;
use crate::core::theme::Theme;
use crate::core::types::{AdoptCandidate, CdmError, Profile, ProfileStatus};
use crate::platform;
use crate::tray;

const CONFIG_FILE: &str = "claude_desktop_config.json";
const RELEASES_URL: &str =
    "https://github.com/namdseasygoingvn/claude-desktop-manager/releases/latest";

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
            CdmError::GroupNotFound(d) => ("groupNotFound", Some(d)),
            CdmError::ProfileRunning(d) => ("profileRunning", Some(d)),
            CdmError::DirExists(d) => ("dirExists", Some(d)),
            CdmError::RegistryCorrupt(d) => ("registryCorrupt", Some(d)),
            CdmError::Io(d) => ("io", Some(d)),
            CdmError::Other(d) => ("other", Some(d)),
        };
        Self { kind, detail }
    }
}

/// The General tab, as one payload. Each field comes from its own owner: the checkbox state
/// is stored, the login item is whatever the OS currently says.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GeneralSettings {
    pub open_preferences_at_start: bool,
    pub show_usage_limits: bool,
    pub launch_at_login: bool,
    pub theme: Theme,
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
pub fn list_groups() -> CmdResult<groups::GroupList> {
    Ok(groups::list()?)
}

#[tauri::command]
pub fn create_group(app: AppHandle, name: String) -> CmdResult<groups::Group> {
    let group = groups::create(&name)?;
    let _ = tray::rebuild(&app);
    Ok(group)
}

#[tauri::command]
pub fn rename_group(app: AppHandle, id: String, new_name: String) -> CmdResult<groups::Group> {
    let group = groups::rename(&id, &new_name)?;
    let _ = tray::rebuild(&app);
    Ok(group)
}

#[tauri::command]
pub fn set_group_icon(
    app: AppHandle,
    id: String,
    icon: Option<groups::GroupIcon>,
) -> CmdResult<groups::Group> {
    let group = groups::set_icon(&id, icon)?;
    let _ = tray::rebuild(&app);
    Ok(group)
}

#[tauri::command]
pub fn delete_group(app: AppHandle, id: String) -> CmdResult<()> {
    groups::delete(&id)?;
    let _ = tray::rebuild(&app);
    Ok(())
}

#[tauri::command]
pub fn set_profile_group(
    app: AppHandle,
    profile_id: String,
    group_id: Option<String>,
) -> CmdResult<()> {
    groups::assign(&profile_id, group_id.as_deref())?;
    let _ = tray::rebuild(&app);
    Ok(())
}

/// Move a profile into a group (or out of one) and place it in the sidebar order before
/// `before`, or last when `before` is None. Powers drag-to-reorder in the sidebar.
#[tauri::command]
pub fn move_profile(
    app: AppHandle,
    profile_id: String,
    group_id: Option<String>,
    before: Option<String>,
) -> CmdResult<()> {
    groups::reposition(&profile_id, group_id.as_deref(), before.as_deref())?;
    let _ = tray::rebuild(&app);
    Ok(())
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
pub fn reveal_profile(id: String) -> CmdResult<()> {
    let dir = profile_dir(&id)?;
    tauri_plugin_opener::reveal_item_in_dir(dir).map_err(|e| CdmError::Io(e.to_string()).into())
}

#[tauri::command]
pub fn get_general_settings(app: AppHandle) -> CmdResult<GeneralSettings> {
    let stored = settings::load();
    Ok(GeneralSettings {
        open_preferences_at_start: stored.open_preferences_at_start,
        show_usage_limits: stored.show_usage_limits,
        // An unreadable login item reads as off: the checkbox then offers to set it.
        launch_at_login: app.autolaunch().is_enabled().unwrap_or(false),
        theme: stored.theme,
    })
}

#[tauri::command]
pub fn set_open_preferences_at_start(enabled: bool) -> CmdResult<()> {
    let mut current = settings::load();
    current.open_preferences_at_start = enabled;
    Ok(settings::save(&current)?)
}

#[tauri::command]
pub fn set_show_usage_limits(app: AppHandle, enabled: bool) -> CmdResult<()> {
    let mut current = settings::load();
    current.show_usage_limits = enabled;
    settings::save(&current)?;
    let _ = tray::rebuild(&app);
    Ok(())
}

/// The webview repaints itself from the same value; this only has to carry it to the frame.
#[tauri::command]
pub fn set_theme(app: AppHandle, theme: Theme) -> CmdResult<()> {
    let mut current = settings::load();
    current.theme = theme;
    settings::save(&current)?;
    tray::apply_theme(&app, theme).map_err(|e| CdmError::Other(e.to_string()))?;
    Ok(())
}

#[tauri::command]
pub fn set_launch_at_login(app: AppHandle, enabled: bool) -> CmdResult<()> {
    let manager = app.autolaunch();
    let outcome = if enabled {
        manager.enable()
    } else {
        manager.disable()
    };
    outcome.map_err(|e| CdmError::Io(e.to_string()).into())
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

#[tauri::command]
pub fn is_translated() -> bool {
    platform::is_translated()
}

#[tauri::command]
pub fn open_releases_page() -> CmdResult<()> {
    tauri_plugin_opener::open_url(RELEASES_URL, None::<&str>)
        .map_err(|e| CdmError::Io(e.to_string()).into())
}

pub fn profile_dir(id: &str) -> CmdResult<PathBuf> {
    let dir = profile::list()?
        .into_iter()
        .find(|s| s.profile.id == id)
        .map(|s| s.profile.dir)
        .ok_or_else(|| CommandError::from(CdmError::ProfileNotFound(id.to_string())))?;
    Ok(profiles_root()?.join(dir))
}

pub fn config_path(id: &str) -> CmdResult<PathBuf> {
    Ok(profile_dir(id)?.join(CONFIG_FILE))
}

pub fn profiles_root() -> CmdResult<PathBuf> {
    Ok(platform::current().profiles_root()?)
}

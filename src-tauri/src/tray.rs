use tauri::menu::{Menu, MenuBuilder, MenuId, MenuItem, MenuItemBuilder};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, Window, WindowEvent, Wry};

use crate::core::profile;
use crate::core::types::ProfileStatus;
use crate::platform;

pub const TRAY_ID: &str = "cdm-tray";
pub const MANAGER_WINDOW: &str = "main";

/// Rows past this open the manager instead; a menu taller than the screen is a broken menu.
const MAX_ROWS: usize = 20;
const RUNNING_MARK: &str = "\u{25cf} ";
const IDLE_MARK: &str = "   ";

const LABEL_NO_PROFILES: &str = "No profiles yet";
const LABEL_BINARY_MISSING: &str = "\u{26a0} Claude Desktop not found";
const LABEL_LOCATE: &str = "Locate Claude Desktop\u{2026}";
const LABEL_REGISTRY_BROKEN: &str = "\u{26a0} Profile list unavailable";
const LABEL_OPEN_MANAGER: &str = "Open Manager to Fix\u{2026}";
const LABEL_NEW: &str = "New Profile\u{2026}";
const LABEL_MANAGE: &str = "Manage Profiles\u{2026}";
const LABEL_MORE: &str = "More\u{2026}";
const LABEL_UPDATE: &str = "Check for Updates\u{2026}";

mod id {
    pub const STATUS: &str = "status";
    pub const EMPTY: &str = "empty";
    pub const LOCATE: &str = "locate";
    pub const NEW: &str = "new";
    pub const MANAGE: &str = "manage";
    pub const MORE: &str = "more";
    pub const QUIT: &str = "quit";
    pub const UPDATE: &str = "update";
    pub const LAUNCH_PREFIX: &str = "launch:";
}

pub mod event {
    pub const NEW_PROFILE: &str = "cdm://new-profile";
    pub const LOCATE_BINARY: &str = "cdm://locate-binary";
    pub const UPDATE_RESULT: &str = "cdm://update-result";
}

pub fn init(app: &AppHandle) -> tauri::Result<()> {
    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .menu(&menu(app)?)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| handle_menu(app, event.id.as_ref()));

    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone()).icon_as_template(true);
    }

    builder.build(app).map(|_| ())
}

/// Call after any profile mutation. Safe from any thread: muda panics if a menu is
/// touched off the main thread on macOS, and async commands do not run there.
pub fn rebuild(app: &AppHandle) -> tauri::Result<()> {
    let handle = app.clone();
    app.run_on_main_thread(move || {
        if let Some(tray) = handle.tray_by_id(TRAY_ID) {
            if let Ok(menu) = menu(&handle) {
                let _ = tray.set_menu(Some(menu));
            }
        }
    })
}

pub fn show_manager(app: &AppHandle) -> tauri::Result<()> {
    // Regular before show(): otherwise the window can come up behind the frontmost app.
    #[cfg(target_os = "macos")]
    {
        app.set_activation_policy(tauri::ActivationPolicy::Regular)?;
    }

    if let Some(window) = app.get_webview_window(MANAGER_WINDOW) {
        window.show()?;
        window.set_focus()?;
    }
    Ok(())
}

/// Wire into `Builder::on_window_event`. `WindowEvent` and `CloseRequested` are both
/// `#[non_exhaustive]`, so this cannot be an exhaustive match.
pub fn on_window_event(window: &Window, event: &WindowEvent) {
    if let WindowEvent::CloseRequested { api, .. } = event {
        if window.label() != MANAGER_WINDOW {
            return;
        }
        api.prevent_close();
        let _ = window.hide();

        #[cfg(target_os = "macos")]
        {
            let _ = window
                .app_handle()
                .set_activation_policy(tauri::ActivationPolicy::Accessory);
        }
    }
}

fn handle_menu(app: &AppHandle, item: &str) {
    match item {
        id::NEW => {
            let _ = show_manager(app);
            let _ = app.emit(event::NEW_PROFILE, ());
        }
        id::LOCATE => {
            let _ = show_manager(app);
            let _ = app.emit(event::LOCATE_BINARY, ());
        }
        id::MANAGE | id::MORE => {
            let _ = show_manager(app);
        }
        id::QUIT => app.exit(0),
        id::UPDATE => {
            let app = app.clone();
            std::thread::spawn(move || {
                let outcome = tauri::async_runtime::block_on(crate::updater::check_for_updates(
                    app.clone(),
                ));
                let _ = app.emit(event::UPDATE_RESULT, outcome.ok());
            });
        }
        other => {
            if let Some(profile_id) = other.strip_prefix(id::LAUNCH_PREFIX) {
                let _ = profile::launch(profile_id);
                let _ = rebuild(app);
            }
        }
    }
}

fn menu(app: &AppHandle) -> tauri::Result<Menu<Wry>> {
    match profile::list() {
        Ok(profiles) => healthy_menu(app, profiles),
        Err(_) => broken_registry_menu(app),
    }
}

fn healthy_menu(app: &AppHandle, mut profiles: Vec<ProfileStatus>) -> tauri::Result<Menu<Wry>> {
    sort_for_display(&mut profiles);
    let binary_ok = platform::current().find_claude_binary().is_ok();

    let status = status_items(app, binary_ok)?;
    let rows = profile_items(app, &profiles, binary_ok)?;

    let mut b = MenuBuilder::new(app);
    for entry in &status {
        b = b.item(entry);
    }
    if !status.is_empty() {
        b = b.separator();
    }
    for entry in &rows {
        b = b.item(entry);
    }
    b.separator()
        .text(id::NEW, LABEL_NEW)
        .text(id::MANAGE, LABEL_MANAGE)
        .separator()
        .text(id::UPDATE, LABEL_UPDATE)
        .text(id::QUIT, quit_label())
        .build()
}

/// `New Profile…` is removed, not disabled: creating a profile writes the registry, and cdm
/// must never write over a file it could not read.
fn broken_registry_menu(app: &AppHandle) -> tauri::Result<Menu<Wry>> {
    let status = item(app, id::STATUS, LABEL_REGISTRY_BROKEN, false)?;
    MenuBuilder::new(app)
        .item(&status)
        .text(id::MANAGE, LABEL_OPEN_MANAGER)
        .separator()
        .text(id::QUIT, quit_label())
        .build()
}

fn status_items(app: &AppHandle, binary_ok: bool) -> tauri::Result<Vec<MenuItem<Wry>>> {
    if binary_ok {
        return Ok(Vec::new());
    }
    Ok(vec![
        item(app, id::STATUS, LABEL_BINARY_MISSING, false)?,
        item(app, id::LOCATE, LABEL_LOCATE, true)?,
    ])
}

fn profile_items(
    app: &AppHandle,
    profiles: &[ProfileStatus],
    enabled: bool,
) -> tauri::Result<Vec<MenuItem<Wry>>> {
    if profiles.is_empty() {
        return Ok(vec![item(app, id::EMPTY, LABEL_NO_PROFILES, false)?]);
    }

    let marked = profiles.iter().any(|p| p.running_pid.is_some());
    let mut items = Vec::with_capacity(profiles.len().min(MAX_ROWS) + 1);
    for p in profiles.iter().take(MAX_ROWS) {
        let row_id = format!("{}{}", id::LAUNCH_PREFIX, p.profile.id);
        items.push(item(app, row_id, row_label(p, marked), enabled)?);
    }
    if profiles.len() > MAX_ROWS {
        items.push(item(app, id::MORE, LABEL_MORE, true)?);
    }
    Ok(items)
}

fn row_label(p: &ProfileStatus, marked: bool) -> String {
    match (marked, p.running_pid.is_some()) {
        (true, true) => format!("{RUNNING_MARK}{}", p.profile.name),
        (true, false) => format!("{IDLE_MARK}{}", p.profile.name),
        _ => p.profile.name.clone(),
    }
}

/// Alphabetical, never most-recently-used: the target must not move under the cursor.
fn sort_for_display(profiles: &mut [ProfileStatus]) {
    profiles.sort_by(|a, b| {
        a.profile
            .name
            .to_lowercase()
            .cmp(&b.profile.name.to_lowercase())
            .then_with(|| a.profile.created_at.cmp(&b.profile.created_at))
    });
}

fn item(
    app: &AppHandle,
    id: impl Into<MenuId>,
    label: impl AsRef<str>,
    enabled: bool,
) -> tauri::Result<MenuItem<Wry>> {
    MenuItemBuilder::with_id(id, label).enabled(enabled).build(app)
}

fn quit_label() -> &'static str {
    if cfg!(target_os = "windows") {
        "Exit"
    } else {
        "Quit Claude Desktop Manager"
    }
}

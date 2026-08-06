use std::collections::HashSet;

use tauri::image::Image;
use tauri::menu::{Menu, MenuBuilder, MenuId, MenuItem, MenuItemBuilder, Submenu};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, Window, WindowEvent, Wry};

use crate::core::groups::{self, Group, GroupIcon};
use crate::core::profile;
use crate::core::settings;
use crate::core::theme::Theme;
use crate::core::types::ProfileStatus;
use crate::core::usage::Usage;
use crate::platform;
use crate::tray_icons;

pub const TRAY_ID: &str = "cdm-tray";
pub const PREFERENCES_WINDOW: &str = "main";

/// Rows past this open Preferences instead; a menu taller than the screen is a broken menu.
const MAX_ROWS: usize = 20;
/// Menu icons render at 18pt on macOS; the 2x bitmap keeps them crisp on retina.
const MENU_ICON_SIZE: u32 = 36;

const LABEL_NO_PROFILES: &str = "No profiles yet";
const LABEL_GROUP_EMPTY: &str = "No profiles in this group";
const LABEL_BINARY_MISSING: &str = "\u{26a0} Claude Desktop not found";
const LABEL_LOCATE: &str = "Locate Claude Desktop\u{2026}";
const LABEL_REGISTRY_BROKEN: &str = "\u{26a0} Profile list unavailable";
const LABEL_OPEN_PREFERENCES: &str = "Open Preferences to Fix\u{2026}";
const LABEL_MORE: &str = "More\u{2026}";
const LABEL_PREFERENCES: &str = "Preferences\u{2026}";
const LABEL_USAGE_SEPARATOR: &str = " \u{2014} ";
const LABEL_USAGE_JOIN: &str = " / ";
const LABEL_USAGE_UNIT: &str = "%";
/// Only fires while cdm is frontmost: an Accessory app has no menu bar to own the key globally.
const PREFERENCES_ACCELERATOR: &str = "CmdOrCtrl+,";

mod id {
    pub const VERSION: &str = "version";
    pub const STATUS: &str = "status";
    pub const EMPTY: &str = "empty";
    pub const LOCATE: &str = "locate";
    pub const PREFERENCES: &str = "preferences";
    pub const MORE: &str = "more";
    pub const QUIT: &str = "quit";
    pub const LAUNCH_PREFIX: &str = "launch:";
    pub const GROUP_PREFIX: &str = "group:";
    /// Suffix on a group's "More…" item; the whole id is `group:<gid>:more`.
    pub const GROUP_MORE_SUFFIX: &str = ":more";
}

pub mod event {
    pub const LOCATE_BINARY: &str = "cdm://locate-binary";
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

/// Paint the native frame to match. `None` hands the window back to the OS, which is what
/// `System` means — there is no third frame colour to ask for.
pub fn apply_theme(app: &AppHandle, theme: Theme) -> tauri::Result<()> {
    let requested = match theme {
        Theme::Light => Some(tauri::Theme::Light),
        Theme::Dark => Some(tauri::Theme::Dark),
        Theme::System => None,
    };
    if let Some(window) = app.get_webview_window(PREFERENCES_WINDOW) {
        window.set_theme(requested)?;
    }
    Ok(())
}

pub fn show_preferences(app: &AppHandle) -> tauri::Result<()> {
    // Regular before show(): otherwise the window can come up behind the frontmost app.
    #[cfg(target_os = "macos")]
    {
        app.set_activation_policy(tauri::ActivationPolicy::Regular)?;
    }

    if let Some(window) = app.get_webview_window(PREFERENCES_WINDOW) {
        window.show()?;
        window.set_focus()?;
    }
    Ok(())
}

/// Wire into `Builder::on_window_event`. `WindowEvent` and `CloseRequested` are both
/// `#[non_exhaustive]`, so this cannot be an exhaustive match.
pub fn on_window_event(window: &Window, event: &WindowEvent) {
    if let WindowEvent::CloseRequested { api, .. } = event {
        if window.label() != PREFERENCES_WINDOW {
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
        id::LOCATE => {
            let _ = show_preferences(app);
            let _ = app.emit(event::LOCATE_BINARY, ());
        }
        id::PREFERENCES | id::MORE => {
            let _ = show_preferences(app);
        }
        id::QUIT => app.exit(0),
        other => {
            if let Some(profile_id) = other.strip_prefix(id::LAUNCH_PREFIX) {
                let _ = profile::launch(profile_id);
                let _ = rebuild(app);
            } else if other.ends_with(id::GROUP_MORE_SUFFIX) {
                let _ = show_preferences(app);
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

    let show_usage = settings::load().show_usage_limits;

    let version = version_item(app)?;
    let status = status_items(app, binary_ok)?;
    // Groups cannot break the tray: one that cannot be read simply does not render.
    let groups = groups::list().unwrap_or_default().groups;
    let group_menus = group_items(app, &groups, &profiles, binary_ok, show_usage)?;
    let rows = ungrouped_items(app, &groups, &profiles, binary_ok, show_usage)?;
    let preferences = preferences_item(app)?;

    let mut b = MenuBuilder::new(app).item(&version).separator();
    for entry in &status {
        b = b.item(entry);
    }
    if !status.is_empty() {
        b = b.separator();
    }
    for submenu in &group_menus {
        b = b.item(submenu);
    }
    for entry in &rows {
        b = b.item(entry);
    }
    b.separator()
        .item(&preferences)
        .text(id::QUIT, quit_label())
        .build()
}

/// Preferences still opens: its Profiles tab is where the rebuild lives. Nothing here writes the
/// registry, because cdm must never write over a file it could not read.
fn broken_registry_menu(app: &AppHandle) -> tauri::Result<Menu<Wry>> {
    let version = version_item(app)?;
    let status = item(app, id::STATUS, LABEL_REGISTRY_BROKEN, false)?;
    MenuBuilder::new(app)
        .item(&version)
        .separator()
        .item(&status)
        .text(id::PREFERENCES, LABEL_OPEN_PREFERENCES)
        .separator()
        .text(id::QUIT, quit_label())
        .build()
}

/// The running version, straight from the bundle: `tauri.conf.json` is its only source.
fn version_item(app: &AppHandle) -> tauri::Result<MenuItem<Wry>> {
    let info = app.package_info();
    item(
        app,
        id::VERSION,
        format!("{} {}", info.name, info.version),
        false,
    )
}

fn preferences_item(app: &AppHandle) -> tauri::Result<MenuItem<Wry>> {
    MenuItemBuilder::with_id(id::PREFERENCES, LABEL_PREFERENCES)
        .accelerator(PREFERENCES_ACCELERATOR)
        .build(app)
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

fn group_items(
    app: &AppHandle,
    groups: &[Group],
    profiles: &[ProfileStatus],
    enabled: bool,
    show_usage: bool,
) -> tauri::Result<Vec<Submenu<Wry>>> {
    groups
        .iter()
        .map(|group| group_menu(app, group, profiles, enabled, show_usage))
        .collect()
}

fn group_menu(
    app: &AppHandle,
    group: &Group,
    profiles: &[ProfileStatus],
    enabled: bool,
    show_usage: bool,
) -> tauri::Result<Submenu<Wry>> {
    // Emoji ride in the label (native color text on every platform); lucide symbols become
    // menu images. The label is what the screen reader and Windows see.
    let label = match &group.icon {
        Some(GroupIcon::Emoji(emoji)) if !emoji.is_empty() => format!("{emoji} {}", group.name),
        _ => group.name.clone(),
    };
    let submenu = Submenu::with_id_and_icon(
        app,
        format!("{}:{}", id::GROUP_PREFIX, group.id),
        label,
        true,
        group_image(group),
    )?;

    let members: Vec<&ProfileStatus> = profiles
        .iter()
        .filter(|status| group.profile_ids.iter().any(|id| id == &status.profile.id))
        .collect();

    if members.is_empty() {
        submenu.append(&item(
            app,
            format!("{}:{}:empty", id::GROUP_PREFIX, group.id),
            LABEL_GROUP_EMPTY,
            false,
        )?)?;
    } else {
        for member in members.iter().take(MAX_ROWS) {
            let row_id = format!("{}{}", id::LAUNCH_PREFIX, member.profile.id);
            submenu.append(&item(app, row_id, row_label(member, show_usage), enabled)?)?;
        }
        if members.len() > MAX_ROWS {
            submenu.append(&item(
                app,
                format!("{}:{}:more", id::GROUP_PREFIX, group.id),
                LABEL_MORE,
                true,
            )?)?;
        }
    }
    Ok(submenu)
}

fn ungrouped_items(
    app: &AppHandle,
    groups: &[Group],
    profiles: &[ProfileStatus],
    enabled: bool,
    show_usage: bool,
) -> tauri::Result<Vec<MenuItem<Wry>>> {
    let grouped: HashSet<&str> = groups
        .iter()
        .flat_map(|group| group.profile_ids.iter().map(String::as_str))
        .collect();
    let ungrouped: Vec<ProfileStatus> = profiles
        .iter()
        .filter(|status| !grouped.contains(status.profile.id.as_str()))
        .cloned()
        .collect();
    profile_items(app, &ungrouped, enabled, show_usage)
}

fn profile_items(
    app: &AppHandle,
    profiles: &[ProfileStatus],
    enabled: bool,
    show_usage: bool,
) -> tauri::Result<Vec<MenuItem<Wry>>> {
    if profiles.is_empty() {
        return Ok(vec![item(app, id::EMPTY, LABEL_NO_PROFILES, false)?]);
    }

    let mut items = Vec::with_capacity(profiles.len().min(MAX_ROWS) + 1);
    for p in profiles.iter().take(MAX_ROWS) {
        let row_id = format!("{}{}", id::LAUNCH_PREFIX, p.profile.id);
        items.push(item(app, row_id, row_label(p, show_usage), enabled)?);
    }
    if profiles.len() > MAX_ROWS {
        items.push(item(app, id::MORE, LABEL_MORE, true)?);
    }
    Ok(items)
}

/// Rasterize a group's lucide icon for the menu. Mid gray so it reads on both menu themes;
/// the tray has no way to tint it per appearance.
fn group_image(group: &Group) -> Option<Image<'static>> {
    let svg = match &group.icon {
        Some(GroupIcon::Symbol(symbol)) => {
            tray_icons::svg(symbol).unwrap_or(tray_icons::DEFAULT_SVG)
        }
        _ => return None,
    };
    let rgba = rasterize(svg)?;
    Some(Image::new_owned(rgba, MENU_ICON_SIZE, MENU_ICON_SIZE))
}

fn rasterize(svg: &str) -> Option<Vec<u8>> {
    let tree = usvg::Tree::from_data(svg.as_bytes(), &usvg::Options::default()).ok()?;
    let scale = MENU_ICON_SIZE as f32 / 24.0;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(MENU_ICON_SIZE, MENU_ICON_SIZE)?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    Some(pixmap.data().to_vec())
}

fn row_label(p: &ProfileStatus, show_usage: bool) -> String {
    let mut label = p.profile.name.clone();
    if show_usage {
        if let Some(suffix) = usage_suffix(p.usage.as_ref()) {
            label.push_str(&suffix);
        }
    }
    label
}

/// Tray rows are one line, so the sample age stays a Preferences detail. A percentage the API
/// never reported is dropped rather than padded with a placeholder.
fn usage_suffix(usage: Option<&Usage>) -> Option<String> {
    let usage = usage?;
    let shown: Vec<String> = [usage.five_hour, usage.seven_day]
        .into_iter()
        .flatten()
        .map(|percent| format!("{percent}{LABEL_USAGE_UNIT}"))
        .collect();
    if shown.is_empty() {
        return None;
    }
    Some(format!(
        "{LABEL_USAGE_SEPARATOR}{}",
        shown.join(LABEL_USAGE_JOIN)
    ))
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

//! The `admin` webview: a locked-down view of claude.ai's org-members page, embedded as a
//! native child of the Preferences window over the Admin tab's pane.

use serde::Deserialize;
use tauri::webview::WebviewBuilder;
use tauri::{AppHandle, LogicalPosition, LogicalSize, Manager, Rect, WebviewUrl};

use crate::tray;

pub const ADMIN_WEBVIEW: &str = "admin";
pub const MEMBERS_PATH: &str = "/admin-settings/members";

/// Logical (CSS) pixels, relative to the window's content area — what the frontend measures.
#[derive(Clone, Copy, Debug, Deserialize)]
pub struct Bounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl From<Bounds> for Rect {
    fn from(bounds: Bounds) -> Self {
        Rect {
            position: LogicalPosition::new(bounds.x, bounds.y).into(),
            size: LogicalSize::new(bounds.width, bounds.height).into(),
        }
    }
}

/// Idempotent: an existing webview is re-positioned and re-shown, not rebuilt. The child does
/// not track the window's size on its own; the frontend re-sends bounds on every layout change.
pub fn show(app: &AppHandle, bounds: Bounds) -> tauri::Result<()> {
    if let Some(webview) = app.get_webview(ADMIN_WEBVIEW) {
        webview.set_bounds(bounds.into())?;
        webview.show()?;
        return Ok(());
    }

    let window = app
        .get_window(tray::PREFERENCES_WINDOW)
        .ok_or(tauri::Error::WindowNotFound)?;

    let url = tauri::Url::parse(&format!("https://claude.ai{MEMBERS_PATH}"))
        .map_err(tauri::Error::InvalidUrl)?;
    let preamble = format!("window.__CDM_MEMBERS_PATH = {MEMBERS_PATH:?};");
    // One call, not three: wry's handling of multiple `initialization_script` registrations is
    // unverified against 2.11.5, and both files read the preamble's global at document-start —
    // if only one call survived, the preamble would be gone and both would silently fail open.
    // Order matters (preamble must define the global first); both files end statements with
    // `;`, so joining with `\n` is safe.
    let init_script = [preamble.as_str(), include_str!("route_lock.js"), include_str!("prune.js")].join("\n");

    let allowed = |url: &tauri::Url| {
        url.scheme() == "https"
            && url
                .host_str()
                .is_some_and(|host| host == "claude.ai" || host.ends_with(".claude.ai"))
    };

    let builder = WebviewBuilder::new(ADMIN_WEBVIEW, WebviewUrl::External(url))
        .initialization_script(init_script)
        .on_navigation(allowed);
    window.add_child(
        builder,
        LogicalPosition::new(bounds.x, bounds.y),
        LogicalSize::new(bounds.width, bounds.height),
    )?;
    Ok(())
}

pub fn hide(app: &AppHandle) -> tauri::Result<()> {
    if let Some(webview) = app.get_webview(ADMIN_WEBVIEW) {
        webview.hide()?;
    }
    Ok(())
}

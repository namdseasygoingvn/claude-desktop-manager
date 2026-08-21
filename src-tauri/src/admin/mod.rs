//! The `admin` window: a locked-down webview onto claude.ai's org-members page.

use tauri::{AppHandle, Manager, Window, WindowEvent};

use crate::tray;

pub const ADMIN_WINDOW: &str = "admin";
pub const MEMBERS_PATH: &str = "/admin-settings/members";

pub fn open(app: &AppHandle) -> tauri::Result<()> {
    // Regular before show(): otherwise the window can come up behind the frontmost app.
    #[cfg(target_os = "macos")]
    {
        app.set_activation_policy(tauri::ActivationPolicy::Regular)?;
    }

    if let Some(window) = app.get_webview_window(ADMIN_WINDOW) {
        window.show()?;
        window.set_focus()?;
        return Ok(());
    }

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

    tauri::WebviewWindowBuilder::new(app, ADMIN_WINDOW, tauri::WebviewUrl::External(url))
        .title("Claude Members")
        .inner_size(980.0, 720.0)
        .initialization_script(init_script)
        .on_navigation(allowed)
        .build()?;
    Ok(())
}

/// Wire into `Builder::on_window_event` alongside `tray::on_window_event`.
pub fn on_window_event(window: &Window, event: &WindowEvent) {
    if let WindowEvent::Destroyed = event {
        if window.label() != ADMIN_WINDOW {
            return;
        }

        #[cfg(target_os = "macos")]
        {
            let app = window.app_handle();
            let main_visible = app
                .get_webview_window(tray::PREFERENCES_WINDOW)
                .is_some_and(|w| w.is_visible().unwrap_or(false));
            if !main_visible {
                let _ = app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            }
        }
    }
}

//! Remembers how large the user left the Preferences window, so a resize survives the next
//! launch instead of snapping back to the size `tauri.conf.json` ships.

use std::sync::mpsc::{self, Sender};
use std::sync::OnceLock;
use std::time::Duration;

use tauri::{AppHandle, LogicalSize, Manager, WebviewWindow, Window};

use crate::core::settings::{self, WindowSize};
use crate::tray::PREFERENCES_WINDOW;

/// A drag fires a `Resized` per frame; only a size that then stays put this long is written.
const SETTLE: Duration = Duration::from_millis(400);

static WRITER: OnceLock<Sender<WindowSize>> = OnceLock::new();

/// Call before the first `show()`: resizing a window already on screen is a visible jump.
pub fn restore(app: &AppHandle, stored: Option<WindowSize>) {
    let Some(stored) = stored else { return };
    let Some(window) = app.get_webview_window(PREFERENCES_WINDOW) else {
        return;
    };
    let size = clamp_to_monitor(&window, stored);
    if let Err(err) = window.set_size(LogicalSize::new(size.width, size.height)) {
        log::warn!("cannot restore the window size: {err}");
        return;
    }
    // `set_size` keeps the top-left corner, so a window larger than the configured default
    // would come up off-centre — and `tauri.conf.json` asks for centred.
    let _ = window.center();
}

/// Wired into `tray::on_window_event`. `Resized` is the one signal that covers every way the
/// app can go away afterwards: hiding the window, tray Quit, Cmd-Q, or a crash.
pub fn remember(window: &Window) {
    // A hidden or minimized window reports a size worth nothing: 0 on Windows, and on macOS
    // the size `restore` itself just applied.
    if !window.is_visible().unwrap_or(false) {
        return;
    }
    let (Ok(scale), Ok(size)) = (window.scale_factor(), window.inner_size()) else {
        return;
    };
    if size.width == 0 || size.height == 0 {
        return;
    }
    let size = size.to_logical::<u32>(scale);
    let _ = writer().send(WindowSize { width: size.width, height: size.height });
}

/// One thread, not one per event: it swallows a whole drag and writes the size it ends on.
fn writer() -> &'static Sender<WindowSize> {
    WRITER.get_or_init(|| {
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            while let Ok(mut size) = rx.recv() {
                while let Ok(newer) = rx.recv_timeout(SETTLE) {
                    size = newer;
                }
                store(size);
            }
        });
        tx
    })
}

fn store(size: WindowSize) {
    let mut current = settings::load();
    if current.window_size == Some(size) {
        return;
    }
    current.window_size = Some(size);
    if let Err(err) = settings::save(&current) {
        log::warn!("cannot save the window size: {err}");
    }
}

/// A size saved on a larger display must not come back taller than the current one: centred,
/// such a window puts its own title bar off-screen where it cannot be grabbed.
fn clamp_to_monitor(window: &WebviewWindow, size: WindowSize) -> WindowSize {
    let Ok(Some(monitor)) = window.current_monitor() else {
        return size;
    };
    let area = monitor.work_area().size.to_logical::<u32>(monitor.scale_factor());
    WindowSize {
        width: size.width.min(area.width),
        height: size.height.min(area.height),
    }
}

//! The `admin` webview: a locked-down view of claude.ai's org-members page, embedded as a
//! native child of the Preferences window over the Admin tab's pane.

use std::io;
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::Duration;

use serde::Deserialize;
use tauri::webview::{PageLoadEvent, WebviewBuilder};
use tauri::{AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, Rect, WebviewUrl, Window};

use crate::tray;

pub const ADMIN_WEBVIEW: &str = "admin";
pub const MEMBERS_PATH: &str = "/admin-settings/members";

/// Emitted when the child's content process dies, or its load never finishes — see `watch`.
pub const FAILED_EVENT: &str = "cdm://admin-webview-failed";

/// WKWebView reports only page-load Started/Finished, never "navigation blocked" — so a
/// cross-host redirect silently cancelled by `on_navigation` would otherwise leave the tab
/// blank forever with no failure signal at all. This is the fallback for that case.
const LOAD_TIMEOUT: Duration = Duration::from_secs(10);

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

#[derive(PartialEq)]
enum Status {
    Loading,
    Ready,
    Failed,
}

struct Health {
    /// Bumped on every fresh load attempt, so a `watch` timeout from an earlier attempt can
    /// tell it is stale once a newer one has started.
    generation: u64,
    status: Status,
}

static HEALTH: OnceLock<Mutex<Health>> = OnceLock::new();

/// A poisoned lock costs at most a missed retry, so recover rather than propagate — the same
/// bargain `mcp::server` makes.
fn health() -> MutexGuard<'static, Health> {
    HEALTH
        .get_or_init(|| Mutex::new(Health { generation: 0, status: Status::Loading }))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn start_loading() -> u64 {
    let mut health = health();
    health.status = Status::Loading;
    health.generation += 1;
    health.generation
}

fn mark_ready() {
    let mut health = health();
    if health.status == Status::Loading {
        health.status = Status::Ready;
    }
}

/// Wired into `Builder::on_web_content_process_terminate` in `main.rs` for this webview's label.
pub fn mark_terminated(app: &AppHandle) {
    health().status = Status::Failed;
    let _ = app.emit(FAILED_EVENT, ());
}

/// Runs on a background thread; fires `FAILED_EVENT` if `generation` is still loading once
/// `LOAD_TIMEOUT` elapses. A no-op once `mark_ready`/`mark_terminated` has moved it on, and
/// harmless if a newer generation has since started (the generation check is stale by then).
fn watch(app: AppHandle, generation: u64) {
    std::thread::spawn(move || {
        std::thread::sleep(LOAD_TIMEOUT);
        let mut health = health();
        if health.generation == generation && health.status == Status::Loading {
            health.status = Status::Failed;
            drop(health);
            let _ = app.emit(FAILED_EVENT, ());
        }
    });
}

/// Clamps to the window's current logical content area — this alone only stops a Rect from
/// spilling past the window's own edges, it has no notion of where the tab strip ends. A Rect
/// that already covers the whole window (x=0, y=0, full width/height — the literal reported bug)
/// is entirely inside the window and passes through unchanged.
fn clamp(bounds: Bounds, window: &Window) -> tauri::Result<Bounds> {
    let scale = window.scale_factor()?;
    let content = window.inner_size()?.to_logical::<f64>(scale);
    let x = bounds.x.max(0.0).min(content.width);
    let y = bounds.y.max(0.0).min(content.height);
    let width = bounds.width.max(0.0).min(content.width - x);
    let height = bounds.height.max(0.0).min(content.height - y);
    reject_implausible(Bounds { x, y, width, height })
}

/// The tab strip always renders above the admin pane (`renderAdmin` in `admin.ts` measures a
/// host below it), so a legitimate Rect never starts flush with y=0. This is the actual backstop
/// against the reported "covers the whole window, including the tab bar" bug: reject a Rect that
/// violates that invariant instead of clamping it through unchanged.
fn reject_implausible(bounds: Bounds) -> tauri::Result<Bounds> {
    if bounds.y <= 0.0 || bounds.width <= 0.0 || bounds.height <= 0.0 {
        return Err(tauri::Error::Io(io::Error::new(
            io::ErrorKind::InvalidInput,
            "admin bounds leave no room for the tab strip",
        )));
    }
    Ok(bounds)
}

/// Idempotent: an existing webview is re-positioned and re-shown, not rebuilt — unless its
/// content process has died (`Status::Failed`), in which case it is reloaded in place. The
/// child does not track the window's size on its own; the frontend re-sends bounds on every
/// layout change.
pub fn show(app: &AppHandle, bounds: Bounds) -> tauri::Result<()> {
    let window = app
        .get_window(tray::PREFERENCES_WINDOW)
        .ok_or(tauri::Error::WindowNotFound)?;
    let bounds = clamp(bounds, &window)?;

    if let Some(webview) = app.get_webview(ADMIN_WEBVIEW) {
        webview.set_bounds(bounds.into())?;
        webview.show()?;
        if health().status == Status::Failed {
            let generation = start_loading();
            webview.reload()?;
            watch(app.clone(), generation);
        }
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

    let generation = start_loading();
    let builder = WebviewBuilder::new(ADMIN_WEBVIEW, WebviewUrl::External(url))
        .initialization_script(init_script)
        .on_navigation(allowed)
        .on_page_load(|_webview, payload| {
            if payload.event() == PageLoadEvent::Finished {
                mark_ready();
            }
        });
    window.add_child(
        builder,
        LogicalPosition::new(bounds.x, bounds.y),
        LogicalSize::new(bounds.width, bounds.height),
    )?;
    watch(app.clone(), generation);
    Ok(())
}

pub fn hide(app: &AppHandle) -> tauri::Result<()> {
    if let Some(webview) = app.get_webview(ADMIN_WEBVIEW) {
        webview.hide()?;
    }
    Ok(())
}

//! The `admin` webview: a locked-down view of claude.ai's org-members page, embedded as a
//! native child of the Preferences window over the Admin tab's pane.

use std::io;
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::Duration;

use serde::Deserialize;
use tauri::webview::{PageLoadEvent, PageLoadPayload, WebviewBuilder};
use tauri::{
    AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, Rect, Webview, WebviewUrl,
};

use crate::tray;

/// Base domains claude.ai's own login flow may redirect through (SSO/IdP hops land on
/// `anthropic.com`, not just `claude.ai`). A host is allowed if it equals a base or is one of
/// its subdomains.
const ALLOWED_HOSTS: [&str; 2] = ["claude.ai", "anthropic.com"];

/// `{scheme}://{host}{path}` — deliberately omits query and fragment so nothing sensitive
/// (auth tokens, codes) ever reaches the log.
fn origin_and_path(url: &tauri::Url) -> String {
    format!("{}://{}{}", url.scheme(), url.host_str().unwrap_or(""), url.path())
}

pub const ADMIN_WEBVIEW: &str = "admin";
pub const MEMBERS_PATH: &str = "/admin-settings/members";

/// Emitted when the child's content process dies, or its load never finishes — see `watch`.
pub const FAILED_EVENT: &str = "cdm://admin-webview-failed";

/// WKWebView reports only page-load Started/Finished, never "navigation blocked" — so a
/// cross-host redirect silently cancelled by `on_navigation` would otherwise leave the tab
/// blank forever with no failure signal at all. This is the fallback for that case.
const LOAD_TIMEOUT: Duration = Duration::from_secs(10);

/// Logical (CSS) pixels in the frontend's own viewport: the rect it measured, plus the height of
/// the viewport it measured in. That height is the only way the title-bar band is knowable — see
/// `titlebar_inset`.
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Bounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub viewport_height: f64,
}

/// Logical pixels in the space a child webview is placed in: origin at the window frame's
/// top-left, which on macOS is above the title bar, not at the content's top edge.
#[derive(Clone, Copy, Debug)]
struct Placement {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

impl From<Placement> for Rect {
    fn from(placement: Placement) -> Self {
        Rect {
            position: LogicalPosition::new(placement.x, placement.y).into(),
            size: LogicalSize::new(placement.width, placement.height).into(),
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
    log::warn!(target: "cdm::admin", "content process terminated");
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
            log::warn!(target: "cdm::admin", "load timed out after {LOAD_TIMEOUT:?}, generation {generation}");
            let _ = app.emit(FAILED_EVENT, ());
        }
    });
}

/// The band the window frame keeps above the frontend's viewport: the title bar on macOS, where
/// tao reports frame and content as the same box (`inner_size == outer_size` and
/// `inner_position == outer_position`), so a child placed at the frontend's own y lands one
/// title-bar height too high and buries the tab strip. Zero on platforms whose inner size
/// already excludes the frame, which is what makes this safe to apply everywhere.
fn titlebar_inset(window_height: f64, viewport_height: f64) -> f64 {
    (window_height - viewport_height).max(0.0)
}

/// Translates the frontend's viewport-space rect into the window-frame space the child is placed
/// in, then clamps it to that frame — which alone only stops a Rect from spilling past the
/// window's own edges; it has no notion of where the tab strip ends.
fn clamp(bounds: Bounds, scale: f64, frame: tauri::LogicalSize<f64>) -> tauri::Result<Placement> {
    let inset = titlebar_inset(frame.height, bounds.viewport_height);
    log::info!(
        target: "cdm::admin",
        "incoming bounds {bounds:?}, window frame {frame:?}, scale {scale}, titlebar inset {inset}"
    );
    let x = bounds.x.max(0.0).min(frame.width);
    let y = (bounds.y + inset).max(0.0).min(frame.height);
    let width = bounds.width.max(0.0).min(frame.width - x);
    let height = bounds.height.max(0.0).min(frame.height - y);
    let placed = reject_implausible(Placement { x, y, width, height }, inset)?;
    log::info!(target: "cdm::admin", "placed bounds {placed:?}");
    Ok(placed)
}

/// A noise floor for sub-pixel/zero-value garbage (e.g. a measurement taken before layout has
/// settled), measured against `inset` so it keeps meaning "left something above it" in the
/// frontend's own space — it is NOT a model of where the tab strip ends. A y a pixel or two past
/// the inset still passes through; the real defenses against a badly-raced measurement are the
/// frontend settle-guard in `admin.ts` and the read-back logging in `log_placement`.
fn reject_implausible(placement: Placement, inset: f64) -> tauri::Result<Placement> {
    if placement.y < inset + 1.0 || placement.width <= 0.0 || placement.height <= 0.0 {
        return Err(tauri::Error::Io(io::Error::new(
            io::ErrorKind::InvalidInput,
            "admin bounds leave no room for the tab strip",
        )));
    }
    Ok(placement)
}

/// Logs expected vs. actually-applied bounds so a placement mismatch (e.g. AppKit not honoring
/// the requested frame) is visible in the log rather than only inferred from a screenshot.
fn log_placement(webview: &Webview, scale: f64, expected: &Placement) {
    match webview.bounds() {
        Ok(actual) => {
            let position = actual.position.to_logical::<f64>(scale);
            let size = actual.size.to_logical::<f64>(scale);
            log::info!(
                target: "cdm::admin",
                "placement expected {expected:?}, actual x={} y={} width={} height={}",
                position.x, position.y, size.width, size.height
            );
        }
        Err(err) => log::warn!(target: "cdm::admin", "could not read back webview bounds: {err}"),
    }
}

/// Idempotent: an existing webview is re-positioned and re-shown, not rebuilt — unless its
/// content process has died (`Status::Failed`), in which case it is reloaded in place. The
/// child does not track the window's size on its own; the frontend re-sends bounds on every
/// layout change.
pub fn show(app: &AppHandle, bounds: Bounds) -> tauri::Result<()> {
    let window = app
        .get_window(tray::PREFERENCES_WINDOW)
        .ok_or(tauri::Error::WindowNotFound)?;
    let scale = window.scale_factor()?;
    let frame = window.inner_size()?.to_logical::<f64>(scale);
    let placement = clamp(bounds, scale, frame)?;

    if let Some(webview) = app.get_webview(ADMIN_WEBVIEW) {
        webview.set_bounds(placement.into())?;
        webview.show()?;
        log_placement(&webview, scale, &placement);
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
    // One call, not two: wry's handling of multiple `initialization_script` registrations is
    // unverified against 2.11.5, and prune.js reads the preamble's global at document-start —
    // if only one call survived, the preamble would be gone and prune.js would silently fail
    // open. Order matters (preamble must define the global first); the file ends statements
    // with `;`, so joining with `\n` is safe.
    let init_script = [preamble.as_str(), include_str!("prune.js")].join("\n");

    let allowed = |url: &tauri::Url| {
        let is_allowed = url.scheme() == "https"
            && url.host_str().is_some_and(|host| {
                ALLOWED_HOSTS
                    .iter()
                    .any(|base| host == *base || host.ends_with(&format!(".{base}")))
            });
        if is_allowed {
            log::info!(target: "cdm::admin", "navigation allowed: {}", origin_and_path(url));
        } else {
            log::warn!(target: "cdm::admin", "navigation denied: {}", origin_and_path(url));
        }
        is_allowed
    };

    let generation = start_loading();
    let builder = WebviewBuilder::new(ADMIN_WEBVIEW, WebviewUrl::External(url))
        .initialization_script(init_script)
        .on_navigation(allowed)
        .on_page_load(|_webview, payload: PageLoadPayload| {
            match payload.event() {
                PageLoadEvent::Started => {
                    log::info!(target: "cdm::admin", "page load started: {}", origin_and_path(payload.url()));
                }
                PageLoadEvent::Finished => {
                    log::info!(target: "cdm::admin", "page load finished: {}", origin_and_path(payload.url()));
                    mark_ready();
                }
            }
        });
    let webview = window.add_child(
        builder,
        LogicalPosition::new(placement.x, placement.y),
        LogicalSize::new(placement.width, placement.height),
    )?;
    // Redundant re-assert: cheap, and covers the case where the native child doesn't reliably
    // honor the creation-time bounds attribute alone.
    webview.set_bounds(placement.into())?;
    log_placement(&webview, scale, &placement);
    watch(app.clone(), generation);
    Ok(())
}

pub fn hide(app: &AppHandle) -> tauri::Result<()> {
    log::info!(target: "cdm::admin", "hide_admin_view");
    if let Some(webview) = app.get_webview(ADMIN_WEBVIEW) {
        webview.hide()?;
    }
    Ok(())
}

/// Forwards Cmd/Ctrl+H when it lands in the main window rather than in the child webview; the
/// toggle state itself lives in prune.js.
pub fn toggle_prune(app: &AppHandle) -> tauri::Result<()> {
    if let Some(webview) = app.get_webview(ADMIN_WEBVIEW) {
        webview.eval("window.__cdmPruneToggle && window.__cdmPruneToggle();")?;
    }
    Ok(())
}

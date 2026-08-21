/// Public so `generate_handler!` can reach the items `#[tauri::command]` generates beside each
/// function; a re-export carries the function alone and the macro then cannot find them.
pub mod commands;

mod client_config;
mod http;
mod logbuf;
mod rpc;
mod server;
mod tools;
#[cfg(target_os = "macos")]
mod tools_debug;
mod tools_state;

use tauri::AppHandle;

pub use logbuf::Sink as LogSink;

/// Matches the committed `.mcp.json` the CLI connects with. Past chrome-profiler (20203)
/// and Claude Quota Monitor (20202, 20204), so every debug server can run at once.
pub const DEFAULT_PORT: u16 = 20205;

/// Below this the OS wants privileges the app does not have, so refusing is kinder than a
/// permission error the user cannot act on.
pub const LOWEST_PORT: u16 = 1024;

pub const SERVER_NAME: &str = "cdm";

/// Overrides both switches for one launch, without touching preferences that outlive it.
/// `off` disables the server, `0` takes any free port.
const PORT_ENV: &str = "CDM_MCP_PORT";

const PATH: &str = "/mcp";

/// `CDM_MCP_PORT` as it was actually set. `port: None` is off — which is also where an
/// unparseable value lands, since guessing at a typo'd port would be worse than staying quiet.
pub struct EnvOverride {
    pub raw: String,
    pub port: Option<u16>,
}

pub fn env_override() -> Option<EnvOverride> {
    let raw = std::env::var(PORT_ENV).ok()?;
    let trimmed = raw.trim();
    let port = if trimmed.eq_ignore_ascii_case("off") {
        None
    } else {
        trimmed.parse().ok()
    };
    Some(EnvOverride {
        raw: trimmed.to_string(),
        port,
    })
}

/// The one place the connection string is spelled, so the status line, `get_app_info`, and
/// whatever the user pastes into `.mcp.json` cannot disagree about the path.
pub fn url(port: u16) -> String {
    format!("http://127.0.0.1:{port}{PATH}")
}

/// Bring the server to whatever the settings and the environment currently ask for: called at
/// startup and again after either switch moves. Failing to bind is never fatal — the app is
/// the product, and `get_mcp_status` carries the reason to the General tab.
pub fn apply(app: &AppHandle) {
    let Some(port) = wanted() else {
        server::stop();
        return;
    };

    match server::start(port, |bound| endpoint(app, bound)) {
        Ok(bound) => {
            log::info!("MCP debug server listening on {}", url(bound));
            // From the bound port, not the requested one, so a client is sent where the
            // server actually is.
            client_config::sync(SERVER_NAME, bound);
        }
        Err(err) => log::warn!("MCP debug server could not bind port {port}: {err}"),
    }
}

/// The port the server should be on, or None for off. The environment outranks the stored
/// switches; with no override the stored ones are the whole answer, in every build.
fn wanted() -> Option<u16> {
    if let Some(over) = env_override() {
        return over.port;
    }
    let stored = crate::core::settings::load();
    stored.mcp_enabled.then_some(stored.mcp_port)
}

fn endpoint(app: &AppHandle, port: u16) -> http::Endpoint {
    http::Endpoint {
        info: rpc::ServerInfo {
            name: SERVER_NAME.to_string(),
            version: app.package_info().version.to_string(),
        },
        tools: tools::build(app, port),
        path: PATH,
    }
}

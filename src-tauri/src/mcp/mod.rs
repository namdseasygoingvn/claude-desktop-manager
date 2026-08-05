mod http;
mod logbuf;
mod rpc;
mod tools;

use tauri::AppHandle;

pub use logbuf::Sink as LogSink;

/// Matches the committed `.mcp.json` the CLI connects with. Past chrome-profiler (20203)
/// and Claude Quota Monitor (20202, 20204), so every debug server can run at once.
pub const DEFAULT_PORT: u16 = 20205;

/// Overrides the port — and in a release build is what turns the server on at all. A
/// loopback endpoint that can delete profiles is a development affordance, not a shipped
/// one. `off` disables it, `0` takes any free port.
const PORT_ENV: &str = "CDM_MCP_PORT";

const PATH: &str = "/mcp";

/// Bring up the embedded MCP debug server so Claude Code can inspect and drive this
/// process while it runs. Failing to bind is never fatal: the app is the product.
pub fn start(app: &AppHandle) {
    let Some(requested) = configured_port() else {
        return;
    };

    let (listener, port) = match http::bind(requested) {
        Ok(bound) => bound,
        Err(err) => {
            log::warn!("MCP debug server could not bind port {requested}: {err}");
            return;
        }
    };

    http::serve(
        listener,
        http::Endpoint {
            info: rpc::ServerInfo {
                name: "cdm".to_string(),
                version: app.package_info().version.to_string(),
            },
            tools: tools::build(app, port),
            path: PATH,
        },
    );

    log::info!("MCP debug server listening on http://127.0.0.1:{port}{PATH}");
}

fn configured_port() -> Option<u16> {
    let Ok(raw) = std::env::var(PORT_ENV) else {
        return cfg!(debug_assertions).then_some(DEFAULT_PORT);
    };

    let raw = raw.trim();
    if raw.eq_ignore_ascii_case("off") {
        return None;
    }
    match raw.parse() {
        Ok(port) => Some(port),
        Err(_) => {
            log::warn!("{PORT_ENV}={raw} is not a port; the MCP debug server stays off");
            None
        }
    }
}

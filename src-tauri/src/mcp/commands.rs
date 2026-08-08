//! The MCP section's IPC surface. Every setter saves first and then re-reads the world, so
//! what comes back is what actually happened rather than what was asked for.

use serde::Serialize;
use tauri::AppHandle;

use super::{logbuf, rpc, server};
use crate::commands::{CmdResult, CommandError};
use crate::core::settings;
use crate::core::types::CdmError;

/// Everything the General tab shows about the debug server. `enabled` is the stored switch;
/// `listening` is whether it took.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpStatus {
    pub enabled: bool,
    /// The stored port — what the field edits. `bound_port` is the one a client can reach,
    /// which differs when the environment overrides it or the port was 0.
    pub port: u16,
    pub bound_port: Option<u16>,
    pub listening: bool,
    pub url: Option<String>,
    pub error: Option<String>,
    /// Set when `CDM_MCP_PORT` is in control, in which case `enabled` and `port` are ignored.
    pub env_override: Option<String>,
    pub name: &'static str,
    pub version: String,
    pub protocol_version: &'static str,
    pub tools: usize,
    pub requests: u64,
    pub uptime_seconds: Option<u64>,
}

#[tauri::command]
pub fn get_mcp_status(app: AppHandle) -> McpStatus {
    status(&app)
}

#[tauri::command]
pub fn set_mcp_enabled(app: AppHandle, enabled: bool) -> CmdResult<McpStatus> {
    let mut current = settings::load();
    current.mcp_enabled = enabled;
    settings::save(&current)?;
    super::apply(&app);
    Ok(status(&app))
}

#[tauri::command]
pub fn set_mcp_port(app: AppHandle, port: u16) -> CmdResult<McpStatus> {
    if port < super::LOWEST_PORT {
        return Err(CommandError::from(CdmError::Other(format!(
            "the port has to be {} or higher",
            super::LOWEST_PORT
        ))));
    }

    let mut current = settings::load();
    current.mcp_port = port;
    settings::save(&current)?;
    super::apply(&app);
    Ok(status(&app))
}

#[tauri::command]
pub fn get_mcp_logs(limit: usize) -> Vec<String> {
    let mut lines = logbuf::snapshot();
    if limit > 0 && lines.len() > limit {
        lines = lines.split_off(lines.len() - limit);
    }
    lines
}

#[tauri::command]
pub fn clear_mcp_logs() {
    logbuf::clear();
}

fn status(app: &AppHandle) -> McpStatus {
    let stored = settings::load();
    let live = server::snapshot();
    let over = super::env_override();

    McpStatus {
        enabled: stored.mcp_enabled,
        port: stored.mcp_port,
        bound_port: live.as_ref().map(|live| live.port),
        listening: live.is_some(),
        url: live.as_ref().map(|live| super::url(live.port)),
        error: server::last_error(),
        env_override: over.map(|over| over.raw),
        name: super::SERVER_NAME,
        version: app.package_info().version.to_string(),
        protocol_version: rpc::PROTOCOL_VERSION,
        tools: live.as_ref().map_or(0, |live| live.tools),
        requests: live.as_ref().map_or(0, |live| live.requests),
        uptime_seconds: live.as_ref().map(|live| live.uptime_seconds),
    }
}

use serde::Serialize;
use serde_json::{json, Value};
use tauri::AppHandle;

use super::logbuf;
use super::rpc::Tool;
use crate::commands::{self, CmdResult, CommandError};
use crate::core::groups;
use crate::core::registry;
use crate::platform;
use crate::tray;

/// Everything the debug server exposes to Claude Code. Each tool is a thin adapter over the
/// same `commands::` function the window calls, so a tool can never drift from the real path.
pub fn build(app: &AppHandle, port: u16) -> Vec<Tool> {
    let mut tools = vec![
        info(app, port),
        logs(),
        list_profiles(),
        get_profile(),
        list_adoptable(),
        read_registry(),
        read_config(),
        doctor(app),
        create_profile(app),
        launch_profile(app),
        quit_profile(app),
        rename_profile(app),
        delete_profile(app),
        adopt_folder(app),
        reveal_profile(),
        open_config(),
        show_preferences(app),
        rebuild_tray(app),
        list_groups(),
        create_group(app),
        rename_group(app),
        set_group_icon(app),
        delete_group(app),
        set_profile_group(app),
    ];
    tools.extend(super::tools_state::all(app));
    #[cfg(target_os = "macos")]
    tools.extend(super::tools_debug::all(app));
    tools
}

fn list_groups() -> Tool {
    tool(
        "list_groups",
        "Every user-defined group with its icon and member profile ids.",
        no_args(),
        |_| out(commands::list_groups().map(|list| list.groups)),
    )
}

fn create_group(app: &AppHandle) -> Tool {
    let app = app.clone();
    tool(
        "create_group",
        "Create an empty group.",
        object(json!({"name": string_prop("Display name for the new group.")}), &["name"]),
        move |args| out(commands::create_group(app.clone(), require_str(args, "name")?)),
    )
}

fn rename_group(app: &AppHandle) -> Tool {
    let app = app.clone();
    tool(
        "rename_group",
        "Rename a group. Members and icon are untouched.",
        object(
            json!({
                "group": string_prop("Group id or name."),
                "name": string_prop("New display name."),
            }),
            &["group", "name"],
        ),
        move |args| {
            let id = resolve_group(args)?;
            out(commands::rename_group(app.clone(), id, require_str(args, "name")?))
        },
    )
}

fn set_group_icon(app: &AppHandle) -> Tool {
    let app = app.clone();
    tool(
        "set_group_icon",
        "Set a group's icon: {\"emoji\":\"🏢\"}, {\"symbol\":\"Folder\"} (a lucide icon name), or null to clear it.",
        object(
            json!({
                "group": string_prop("Group id or name."),
                "icon": json!({
                    "type": ["object", "null"],
                    "properties": {
                        "emoji": string_prop("An emoji character."),
                        "symbol": string_prop("A lucide icon name, e.g. \"Folder\"."),
                    },
                }),
            }),
            &["group", "icon"],
        ),
        move |args| {
            let id = resolve_group(args)?;
            let icon = match args.get("icon") {
                Some(Value::Null) | None => None,
                Some(value) => Some(
                    serde_json::from_value(value.clone())
                        .map_err(|e| format!("icon: {e}"))?,
                ),
            };
            out(commands::set_group_icon(app.clone(), id, icon))
        },
    )
}

fn delete_group(app: &AppHandle) -> Tool {
    let app = app.clone();
    tool(
        "delete_group",
        "Delete a group. Its profiles stay in the list, ungrouped.",
        object(json!({"group": string_prop("Group id or name.")}), &["group"]),
        move |args| {
            let id = resolve_group(args)?;
            commands::delete_group(app.clone(), id.clone()).map_err(detail)?;
            Ok(json!({"deleted": id}))
        },
    )
}

fn set_profile_group(app: &AppHandle) -> Tool {
    let app = app.clone();
    tool(
        "set_profile_group",
        "Move a profile into one group, removing it from every other. Pass group: null to ungroup it.",
        object(
            json!({
                "profile": string_prop("Profile id or name."),
                "group": json!({"type": ["string", "null"], "description": "Group id or name, or null to ungroup."}),
            }),
            &["profile", "group"],
        ),
        move |args| {
            let profile = resolve_id(args)?;
            let group = match args.get("group") {
                Some(Value::Null) | None => None,
                Some(_) => Some(find_group(&require_str(args, "group")?)?.id),
            };
            commands::set_profile_group(app.clone(), profile, group).map_err(detail)?;
            Ok(json!({"assigned": true}))
        },
    )
}

fn info(app: &AppHandle, port: u16) -> Tool {
    let app = app.clone();
    tool(
        "get_app_info",
        "Version, resolved Claude Desktop binary, profiles root, registry path, and profile counts. Start here.",
        no_args(),
        move |_| {
            let platform = platform::current();
            let (binary, binary_error) = match platform.find_claude_binary() {
                Ok(path) => (json!(path.display().to_string()), Value::Null),
                Err(err) => (Value::Null, json!(err.to_string())),
            };
            let profiles = commands::list_profiles().unwrap_or_default();
            Ok(json!({
                "name": "cdm",
                "version": app.package_info().version.to_string(),
                "os": std::env::consts::OS,
                "port": port,
                "connectionUrl": super::url(port),
                "binary": binary,
                "binaryError": binary_error,
                "profilesRoot": path_or_error(commands::profiles_root()),
                "registryPath": path_or_error(registry::path().map_err(CommandError::from)),
                "counts": {
                    "profiles": profiles.len(),
                    "running": profiles.iter().filter(|s| s.running_pid.is_some()).count(),
                    "adoptable": commands::list_adoptable().map(|c| c.len()).unwrap_or(0),
                },
            }))
        },
    )
}

fn logs() -> Tool {
    tool(
        "get_logs",
        "The app's in-memory debug log (every log record plus MCP requests). Read this first when diagnosing. Optional: limit (tail N lines), clear (wipe after reading).",
        json!({
            "type": "object",
            "properties": {
                "limit": {"type": "integer", "description": "Return only the last N lines."},
                "clear": {"type": "boolean", "description": "Clear the buffer after returning it."},
            },
            "additionalProperties": false,
        }),
        |args| {
            let mut lines = logbuf::snapshot();
            if let Some(limit) = args.get("limit").and_then(Value::as_u64) {
                let limit = limit as usize;
                if limit > 0 && lines.len() > limit {
                    lines = lines.split_off(lines.len() - limit);
                }
            }
            if args.get("clear").and_then(Value::as_bool) == Some(true) {
                logbuf::clear();
            }
            Ok(json!({"count": lines.len(), "lines": lines}))
        },
    )
}

fn list_profiles() -> Tool {
    tool(
        "list_profiles",
        "Every registered profile with its folder, timestamps, and running pid.",
        no_args(),
        |_| out(commands::list_profiles()),
    )
}

fn get_profile() -> Tool {
    tool(
        "get_profile",
        "One profile with its resolved folder path and config path. Identify it by id or by name.",
        object(json!({"profile": string_prop("Profile id or name.")}), &["profile"]),
        |args| {
            let reference = require_str(args, "profile")?;
            let status = find(&reference)?;
            let mut value = to_value(&status)?;
            value["dirPath"] = path_or_error(commands::profile_dir(&status.profile.id));
            value["configPath"] = path_or_error(commands::config_path(&status.profile.id));
            Ok(value)
        },
    )
}

fn list_adoptable() -> Tool {
    tool(
        "list_adoptable",
        "Hand-made profile folders under the profiles root that are not registered yet.",
        no_args(),
        |_| out(commands::list_adoptable()),
    )
}

fn read_registry() -> Tool {
    tool(
        "read_registry",
        "The raw registry.json cdm persists — the source of truth behind list_profiles.",
        no_args(),
        |_| {
            let path = registry::path().map_err(|e| e.to_string())?;
            read_json(&path)
        },
    )
}

fn read_config() -> Tool {
    tool(
        "read_config",
        "A profile's claude_desktop_config.json (its MCP servers and settings), by profile id or name.",
        object(json!({"profile": string_prop("Profile id or name.")}), &["profile"]),
        |args| {
            let reference = require_str(args, "profile")?;
            let path = commands::config_path(&find(&reference)?.profile.id).map_err(detail)?;
            read_json(&path)
        },
    )
}

fn doctor(app: &AppHandle) -> Tool {
    let app = app.clone();
    tool(
        "doctor",
        "Run the diagnostics report: binary lookup, profiles root, and a registry reconciliation pass.",
        no_args(),
        move |_| out(commands::doctor(app.clone())),
    )
}

fn create_profile(app: &AppHandle) -> Tool {
    let app = app.clone();
    tool(
        "create_profile",
        "Create a profile: scaffolds its folder and registers it. Does not launch it.",
        object(json!({"name": string_prop("Display name for the new profile.")}), &["name"]),
        move |args| out(commands::create_profile(app.clone(), require_str(args, "name")?)),
    )
}

fn launch_profile(app: &AppHandle) -> Tool {
    let app = app.clone();
    tool(
        "launch_profile",
        "Start Claude Desktop for a profile and return its pid. The core action — use it to reproduce launch behavior on demand.",
        object(json!({"profile": string_prop("Profile id or name.")}), &["profile"]),
        move |args| {
            let id = resolve_id(args)?;
            let pid = commands::launch_profile(app.clone(), id.clone()).map_err(detail)?;
            Ok(json!({"launched": id, "pid": pid}))
        },
    )
}

fn quit_profile(app: &AppHandle) -> Tool {
    let app = app.clone();
    tool(
        "quit_profile",
        "Ask a running profile's Claude Desktop to quit.",
        object(json!({"profile": string_prop("Profile id or name.")}), &["profile"]),
        move |args| {
            let id = resolve_id(args)?;
            commands::quit_profile(app.clone(), id.clone()).map_err(detail)?;
            Ok(json!({"quit": id}))
        },
    )
}

fn rename_profile(app: &AppHandle) -> Tool {
    let app = app.clone();
    tool(
        "rename_profile",
        "Rename a profile. The folder on disk is left alone; only the display name changes.",
        object(
            json!({
                "profile": string_prop("Profile id or name."),
                "name": string_prop("New display name."),
            }),
            &["profile", "name"],
        ),
        move |args| {
            let id = resolve_id(args)?;
            out(commands::rename_profile(
                app.clone(),
                id,
                require_str(args, "name")?,
            ))
        },
    )
}

fn delete_profile(app: &AppHandle) -> Tool {
    let app = app.clone();
    tool(
        "delete_profile",
        "Delete a profile: its folder goes to the Trash and it leaves the registry. Refuses while it is running, and always refuses for the default Claude install.",
        object(json!({"profile": string_prop("Profile id or name.")}), &["profile"]),
        move |args| {
            let id = resolve_id(args)?;
            commands::delete_profile(app.clone(), id.clone()).map_err(detail)?;
            Ok(json!({"deleted": id}))
        },
    )
}

fn adopt_folder(app: &AppHandle) -> Tool {
    let app = app.clone();
    tool(
        "adopt_folder",
        "Register an existing folder from list_adoptable as a profile.",
        object(
            json!({
                "dirName": string_prop("Folder name under the profiles root."),
                "name": string_prop("Display name to register it under."),
            }),
            &["dirName", "name"],
        ),
        move |args| {
            out(commands::adopt_folder(
                app.clone(),
                require_str(args, "dirName")?,
                require_str(args, "name")?,
            ))
        },
    )
}

fn reveal_profile() -> Tool {
    tool(
        "reveal_profile",
        "Show a profile's folder in Finder/Explorer. Exercises the live reveal path.",
        object(json!({"profile": string_prop("Profile id or name.")}), &["profile"]),
        |args| {
            let id = resolve_id(args)?;
            commands::reveal_profile(id.clone()).map_err(detail)?;
            Ok(json!({"revealed": id}))
        },
    )
}

fn open_config() -> Tool {
    tool(
        "open_config",
        "Open a profile's claude_desktop_config.json in the default editor.",
        object(json!({"profile": string_prop("Profile id or name.")}), &["profile"]),
        |args| {
            let id = resolve_id(args)?;
            commands::open_config(id.clone()).map_err(detail)?;
            Ok(json!({"opened": id}))
        },
    )
}

fn show_preferences(app: &AppHandle) -> Tool {
    let app = app.clone();
    tool(
        "show_preferences",
        "Bring the Preferences window to the front, as the tray menu does.",
        no_args(),
        move |_| {
            let handle = app.clone();
            app.run_on_main_thread(move || {
                let _ = tray::show_preferences(&handle);
            })
            .map_err(|e| e.to_string())?;
            Ok(json!({"shown": true}))
        },
    )
}

fn rebuild_tray(app: &AppHandle) -> Tool {
    let app = app.clone();
    tool(
        "rebuild_tray",
        "Rebuild the tray menu from the current registry — use it after editing profiles outside the app.",
        no_args(),
        move |_| {
            tray::rebuild(&app).map_err(|e| e.to_string())?;
            Ok(json!({"rebuilt": true}))
        },
    )
}

pub(super) fn tool(
    name: &'static str,
    description: &'static str,
    input_schema: Value,
    handler: impl Fn(&Value) -> Result<Value, String> + Send + Sync + 'static,
) -> Tool {
    Tool {
        name,
        description,
        input_schema,
        handler: Box::new(handler),
    }
}

pub(super) fn no_args() -> Value {
    json!({"type": "object", "properties": {}, "additionalProperties": false})
}

pub(super) fn object(properties: Value, required: &[&str]) -> Value {
    json!({
        "type": "object",
        "properties": properties,
        "required": required,
        "additionalProperties": false,
    })
}

pub(super) fn string_prop(description: &str) -> Value {
    json!({"type": "string", "description": description})
}

pub(super) fn require_str(args: &Value, key: &str) -> Result<String, String> {
    args.get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| format!("missing required argument: {key}"))
}

/// Tools take a profile id *or* name, because a human debugging by hand knows the name.
fn resolve_id(args: &Value) -> Result<String, String> {
    Ok(find(&require_str(args, "profile")?)?.profile.id)
}

/// Groups resolve by id or name, like profiles do.
fn resolve_group(args: &Value) -> Result<String, String> {
    Ok(find_group(&require_str(args, "group")?)?.id)
}

fn find_group(reference: &str) -> Result<groups::Group, String> {
    let groups = commands::list_groups().map_err(detail)?.groups;
    groups
        .iter()
        .find(|group| group.id == reference)
        .or_else(|| {
            groups
                .iter()
                .find(|group| group.name.eq_ignore_ascii_case(reference))
        })
        .cloned()
        .ok_or_else(|| format!("no group with id or name {reference}"))
}

pub(super) fn find(reference: &str) -> Result<crate::core::types::ProfileStatus, String> {
    let profiles = commands::list_profiles().map_err(detail)?;
    profiles
        .iter()
        .find(|status| status.profile.id == reference)
        .or_else(|| {
            profiles
                .iter()
                .find(|status| status.profile.name.eq_ignore_ascii_case(reference))
        })
        .cloned()
        .ok_or_else(|| format!("no profile with id or name {reference}"))
}

fn read_json(path: &std::path::Path) -> Result<Value, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("{} is not valid JSON: {e}", path.display()))
}

pub(super) fn out<T: Serialize>(result: CmdResult<T>) -> Result<Value, String> {
    to_value(&result.map_err(detail)?)
}

pub(super) fn to_value<T: Serialize>(value: &T) -> Result<Value, String> {
    serde_json::to_value(value).map_err(|e| e.to_string())
}

pub(super) fn path_or_error(result: CmdResult<std::path::PathBuf>) -> Value {
    match result {
        Ok(path) => json!(path.display().to_string()),
        Err(err) => json!({"error": detail(err)}),
    }
}

pub(super) fn detail(error: CommandError) -> String {
    match error.detail {
        Some(detail) => format!("{}: {detail}", error.kind),
        None => error.kind.to_string(),
    }
}

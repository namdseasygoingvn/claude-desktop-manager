//! Tools over the app's own state — preferences, plan usage, updates, and the debug server
//! itself. Separate from `tools.rs` so that file stays the profile-and-group table it already is.

use serde_json::{json, Value};
use tauri::AppHandle;

use super::rpc::Tool;
use super::tools::{
    detail, find, no_args, object, path_or_error, require_str, string_prop, to_value, tool,
};
use crate::commands::{self, CommandError};
use crate::core::settings;
use crate::core::theme::Theme;
use crate::updater;

pub fn all(app: &AppHandle) -> Vec<Tool> {
    vec![
        get_settings(app),
        set_theme(app),
        set_open_preferences_at_start(),
        set_show_usage_limits(app),
        set_launch_at_login(app),
        get_usage(),
        check_for_updates(app),
        mcp_status(app),
    ]
}

fn get_settings(app: &AppHandle) -> Tool {
    let app = app.clone();
    tool(
        "get_settings",
        "Every stored preference — theme, open-at-start, usage limits, sidebar width, window size, and the MCP debug server — plus the login item the OS owns and the file the rest live in.",
        no_args(),
        move |_| {
            let mut value = to_value(&settings::load())?;
            value["launchAtLogin"] = json!(commands::get_general_settings(app.clone())
                .map(|general| general.launch_at_login)
                .unwrap_or(false));
            value["path"] = path_or_error(settings::path().map_err(CommandError::from));
            Ok(value)
        },
    )
}

fn set_theme(app: &AppHandle) -> Tool {
    let app = app.clone();
    tool(
        "set_theme",
        "Set the app appearance. The tray and the window both repaint from this.",
        object(
            json!({"theme": {"type": "string", "enum": ["light", "dark", "system"]}}),
            &["theme"],
        ),
        move |args| {
            let requested = require_str(args, "theme")?;
            let theme: Theme = serde_json::from_value(json!(requested))
                .map_err(|_| format!("theme has to be light, dark, or system, not {requested}"))?;
            commands::set_theme(app.clone(), theme).map_err(detail)?;
            Ok(json!({"theme": theme}))
        },
    )
}

fn set_open_preferences_at_start() -> Tool {
    tool(
        "set_open_preferences_at_start",
        "Whether the Preferences window opens when the app starts, or it starts in the tray alone.",
        toggle_args("On to open the window at startup."),
        |args| {
            let enabled = require_bool(args, "enabled")?;
            commands::set_open_preferences_at_start(enabled).map_err(detail)?;
            Ok(json!({"openPreferencesAtStart": enabled}))
        },
    )
}

fn set_show_usage_limits(app: &AppHandle) -> Tool {
    let app = app.clone();
    tool(
        "set_show_usage_limits",
        "Whether each profile's 5-hour and weekly usage shows in the tray and the window.",
        toggle_args("On to show the usage readings."),
        move |args| {
            let enabled = require_bool(args, "enabled")?;
            commands::set_show_usage_limits(app.clone(), enabled).map_err(detail)?;
            Ok(json!({"showUsageLimits": enabled}))
        },
    )
}

fn set_launch_at_login(app: &AppHandle) -> Tool {
    let app = app.clone();
    tool(
        "set_launch_at_login",
        "Add or remove the login item that starts the manager when the user signs in. This one lives in the OS, not in settings.json.",
        toggle_args("On to register the login item."),
        move |args| {
            let enabled = require_bool(args, "enabled")?;
            commands::set_launch_at_login(app.clone(), enabled).map_err(detail)?;
            Ok(json!({"launchAtLogin": enabled}))
        },
    )
}

fn get_usage() -> Tool {
    tool(
        "get_usage",
        "The newest 5-hour and weekly plan-usage readings, for every profile or one named one. A percentage is null when that profile has never reported it — never zero.",
        json!({
            "type": "object",
            "properties": {
                "profile": string_prop("Profile id or name. Omit for every profile."),
            },
            "additionalProperties": false,
        }),
        |args| {
            let profiles = match args.get("profile") {
                Some(Value::Null) | None => commands::list_profiles().map_err(detail)?,
                Some(_) => vec![find(&require_str(args, "profile")?)?],
            };
            let rows: Vec<Value> = profiles
                .iter()
                .map(|status| {
                    json!({
                        "id": status.profile.id,
                        "name": status.profile.name,
                        "running": status.running_pid.is_some(),
                        "usage": status.usage,
                    })
                })
                .collect();
            Ok(json!({"count": rows.len(), "profiles": rows}))
        },
    )
}

fn check_for_updates(app: &AppHandle) -> Tool {
    let app = app.clone();
    tool(
        "check_for_updates",
        "Ask the update server whether a newer version exists. Looks only: installing stays a deliberate click in the Updates tab, so this can never restart the app under you.",
        no_args(),
        move |_| {
            let outcome = tauri::async_runtime::block_on(updater::check_for_updates(app.clone()))
                .map_err(detail)?;
            let mut value = to_value(&outcome)?;
            value["running"] = json!(app.package_info().version.to_string());
            Ok(value)
        },
    )
}

fn mcp_status(app: &AppHandle) -> Tool {
    let app = app.clone();
    tool(
        "get_mcp_status",
        "This debug server's own state: whether it is listening, on which port and URL, its protocol version, how many tools it serves, requests handled, and uptime.",
        no_args(),
        move |_| to_value(&super::commands::get_mcp_status(app.clone())),
    )
}

fn toggle_args(description: &str) -> Value {
    object(
        json!({"enabled": {"type": "boolean", "description": description}}),
        &["enabled"],
    )
}

fn require_bool(args: &Value, key: &str) -> Result<bool, String> {
    args.get(key)
        .and_then(Value::as_bool)
        .ok_or_else(|| format!("missing required argument: {key}"))
}

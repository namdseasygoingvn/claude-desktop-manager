use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::persist;
use super::theme::Theme;
use super::types::Result;
use crate::mcp;
use crate::platform;

pub const SETTINGS_FILE: &str = "settings.json";

/// App-wide preferences. Launch-at-login is deliberately not here: the OS owns that fact and
/// the autostart plugin is the only thing that reads or writes it.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub open_preferences_at_start: bool,
    pub show_usage_limits: bool,
    pub theme: Theme,
    /// None until the divider is dragged, so the stylesheet owns the starting width alone.
    pub sidebar_width: Option<u32>,
    pub mcp_enabled: bool,
    pub mcp_port: u16,
    /// Chosen via Locate Claude Desktop; a stale path is skipped, not an error.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub claude_binary: Option<PathBuf>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            open_preferences_at_start: true,
            show_usage_limits: true,
            theme: Theme::default(),
            sidebar_width: None,
            // Off: serving the app's state over HTTP is opt-in.
            mcp_enabled: false,
            mcp_port: mcp::DEFAULT_PORT,
            claude_binary: None,
        }
    }
}

pub fn path() -> Result<PathBuf> {
    Ok(platform::current().manager_data_dir()?.join(SETTINGS_FILE))
}

/// Cannot fail: a missing or unreadable file is the defaults. Startup reads this, and no
/// preference is worth refusing to launch over.
pub fn load() -> Settings {
    let Ok(path) = path() else {
        return Settings::default();
    };
    let Ok(bytes) = fs::read(path) else {
        return Settings::default();
    };
    serde_json::from_slice(&bytes).unwrap_or_default()
}

pub fn save(settings: &Settings) -> Result<()> {
    let dir = platform::current().manager_data_dir()?;
    persist::write_json(&dir, SETTINGS_FILE, settings, "settings")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_install_opens_preferences() {
        assert!(Settings::default().open_preferences_at_start);
    }

    #[test]
    fn a_file_written_by_an_older_build_falls_back_to_the_defaults() {
        let parsed: Settings = serde_json::from_str(r#"{"somethingElse":1}"#).unwrap();
        assert_eq!(parsed, Settings::default());
    }

    #[test]
    fn the_stored_key_is_camel_case() {
        let settings = Settings {
            open_preferences_at_start: false,
            show_usage_limits: false,
            theme: Theme::Dark,
            sidebar_width: Some(320),
            mcp_enabled: false,
            mcp_port: 20209,
            claude_binary: None,
        };
        let json = serde_json::to_string(&settings).unwrap();
        assert_eq!(
            json,
            r#"{"openPreferencesAtStart":false,"showUsageLimits":false,"theme":"dark","sidebarWidth":320,"mcpEnabled":false,"mcpPort":20209}"#
        );
    }

    #[test]
    fn a_file_from_before_the_divider_existed_has_no_width() {
        let parsed: Settings = serde_json::from_str(r#"{"theme":"dark"}"#).unwrap();
        assert_eq!(parsed.sidebar_width, None);
    }

    #[test]
    fn a_file_from_before_the_debug_server_had_a_switch_leaves_it_off() {
        let parsed: Settings = serde_json::from_str(r#"{"theme":"dark"}"#).unwrap();
        assert!(!parsed.mcp_enabled);
        assert_eq!(parsed.mcp_port, mcp::DEFAULT_PORT);
    }
}

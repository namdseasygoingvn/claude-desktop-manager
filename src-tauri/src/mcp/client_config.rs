//! The `.mcp.json` a client reads to find this server. Rewritten from the port actually bound,
//! so editing the port in Preferences cannot leave the file pointing where nothing answers.

use std::path::{Path, PathBuf};

use serde_json::Value;

const FILE: &str = ".mcp.json";
const SERVERS: &str = "mcpServers";
const URL: &str = "url";

/// The checkout this binary was compiled in. Baked at build time rather than found at run time:
/// a bundled app has no meaningful working directory, and a tree that has since moved or was
/// built somewhere else simply fails the read below and is left alone.
fn path() -> Option<PathBuf> {
    Some(Path::new(env!("CARGO_MANIFEST_DIR")).parent()?.join(FILE))
}

/// Point the config at `port`. A missing file, unreadable JSON, or no entry under our own name
/// all mean there is nothing here to keep in step — none of which is worth bothering the user
/// about, since the server itself is already up.
pub fn sync(name: &str, port: u16) {
    let Some(path) = path() else { return };
    let Ok(text) = std::fs::read_to_string(&path) else {
        return;
    };

    let url = super::url(port);
    let Some(updated) = retarget(&text, name, &url) else {
        return;
    };

    match std::fs::write(&path, updated) {
        Ok(()) => log::info!("{FILE} now points at {url}"),
        Err(err) => log::warn!("cannot point {FILE} at {url}: {err}"),
    }
}

/// The file's new contents, or None when it already says this, has no entry of ours, or is not
/// something we can safely rewrite.
fn retarget(text: &str, name: &str, url: &str) -> Option<String> {
    let mut config: Value = serde_json::from_str(text).ok()?;

    let entry = config.get_mut(SERVERS)?.get_mut(name)?;
    if entry.get(URL).and_then(Value::as_str) == Some(url) {
        return None;
    }
    entry[URL] = Value::String(url.to_string());

    let mut out = serde_json::to_string_pretty(&config).ok()?;
    out.push('\n');
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = concat!(
        "{\n",
        "  \"mcpServers\": {\n",
        "    \"cdm\": {\n",
        "      \"type\": \"http\",\n",
        "      \"url\": \"http://127.0.0.1:20205/mcp\"\n",
        "    }\n",
        "  }\n",
        "}\n"
    );

    /// The committed file is the fixture, so a rewrite has to come back byte-identical apart
    /// from the port — otherwise every port change would also reformat a tracked file.
    #[test]
    fn only_the_port_moves() {
        let out = retarget(FIXTURE, "cdm", "http://127.0.0.1:20209/mcp").expect("a rewrite");
        assert_eq!(out, FIXTURE.replace("20205", "20209"));
    }

    #[test]
    fn a_file_that_already_says_it_is_left_alone() {
        assert!(retarget(FIXTURE, "cdm", "http://127.0.0.1:20205/mcp").is_none());
    }

    #[test]
    fn an_entry_we_do_not_own_is_never_invented() {
        assert!(retarget(FIXTURE, "somethingelse", "http://127.0.0.1:20209/mcp").is_none());
        assert!(retarget(r#"{"other": 1}"#, "cdm", "http://127.0.0.1:1/mcp").is_none());
    }

    #[test]
    fn a_damaged_file_is_never_rewritten() {
        assert!(retarget("{ not json", "cdm", "http://127.0.0.1:1/mcp").is_none());
    }

    /// Anything the user added alongside our entry has to survive the round trip.
    #[test]
    fn a_neighbouring_server_is_kept() {
        let text = r#"{"mcpServers":{"cdm":{"type":"http","url":"http://127.0.0.1:20205/mcp"},"other":{"command":"run"}}}"#;
        let out = retarget(text, "cdm", "http://127.0.0.1:20209/mcp").expect("a rewrite");
        assert!(out.contains("\"other\""));
        assert!(out.contains("\"command\": \"run\""));
        assert!(out.contains("20209"));
    }
}

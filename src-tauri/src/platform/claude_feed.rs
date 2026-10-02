//! Parsing of Claude's MSIX update feed response.

use serde::Deserialize;

const FEED_HOST: &str = "https://api.anthropic.com";
const DEVICE_ID: &str = "claude-desktop-manager";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct LatestRelease {
    pub version: String,
    pub url: String,
}

#[derive(Deserialize)]
struct Feed {
    #[serde(rename = "currentRelease")]
    current_release: String,
    releases: Vec<Release>,
}

#[derive(Deserialize)]
struct Release {
    version: String,
    #[serde(rename = "updateTo")]
    update_to: UpdateTo,
}

#[derive(Deserialize)]
struct UpdateTo {
    url: String,
}

pub(super) fn feed_url(arch: &str) -> String {
    format!("{FEED_HOST}/api/desktop/win32/{arch}/msix/update?device_id={DEVICE_ID}")
}

pub(super) fn parse(json: &str) -> Option<LatestRelease> {
    let feed: Feed = serde_json::from_str(json).ok()?;
    let release = feed
        .releases
        .into_iter()
        .find(|r| r.version == feed.current_release)?;
    if release.update_to.url.is_empty() {
        return None;
    }
    Some(LatestRelease {
        version: feed.current_release,
        url: release.update_to.url,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{"currentRelease":"2.19675.0","releases":[{"version":"2.19675.0","updateTo":{"name":"Claude 2.19675.0","version":"2.19675.0","pub_date":"2026-10-01T00:00:00Z","url":"https://downloads.claude.ai/releases/win32/x64/2.19675.0/Claude-abc123.msix","notes":"fixes"}}]}"#;

    #[test]
    fn the_verified_sample_gives_version_and_url() {
        assert_eq!(
            parse(SAMPLE),
            Some(LatestRelease {
                version: "2.19675.0".into(),
                url: "https://downloads.claude.ai/releases/win32/x64/2.19675.0/Claude-abc123.msix"
                    .into(),
            })
        );
    }

    #[test]
    fn unknown_fields_are_ignored() {
        let json = r#"{"extra":1,"currentRelease":"1.0.0","releases":[{"version":"1.0.0","other":[],"updateTo":{"url":"https://x/y.msix","surprise":true}}]}"#;
        assert_eq!(parse(json).map(|r| r.version), Some("1.0.0".into()));
    }

    #[test]
    fn the_url_comes_from_the_current_release_entry() {
        let json = r#"{"currentRelease":"2.0.0","releases":[
            {"version":"1.0.0","updateTo":{"url":"https://x/old.msix"}},
            {"version":"2.0.0","updateTo":{"url":"https://x/new.msix"}}]}"#;
        assert_eq!(
            parse(json).map(|r| r.url),
            Some("https://x/new.msix".into())
        );
    }

    #[test]
    fn a_current_release_missing_from_releases_gives_none() {
        let json = r#"{"currentRelease":"3.0.0","releases":[{"version":"2.0.0","updateTo":{"url":"https://x/a.msix"}}]}"#;
        assert_eq!(parse(json), None);
    }

    #[test]
    fn an_empty_url_gives_none() {
        let json =
            r#"{"currentRelease":"1.0.0","releases":[{"version":"1.0.0","updateTo":{"url":""}}]}"#;
        assert_eq!(parse(json), None);
    }

    #[test]
    fn a_missing_current_release_gives_none() {
        let json = r#"{"releases":[{"version":"1.0.0","updateTo":{"url":"https://x/a.msix"}}]}"#;
        assert_eq!(parse(json), None);
    }

    #[test]
    fn an_error_body_gives_none() {
        let json = r#"{"type":"error","error":{"type":"invalid_request_error","message":"device_id: Field required"}}"#;
        assert_eq!(parse(json), None);
    }

    #[test]
    fn non_json_gives_none() {
        assert_eq!(parse("<html>502 Bad Gateway</html>"), None);
        assert_eq!(parse(""), None);
    }

    #[test]
    fn the_feed_url_is_exact() {
        assert_eq!(
            feed_url("arm64"),
            "https://api.anthropic.com/api/desktop/win32/arm64/msix/update?device_id=claude-desktop-manager"
        );
    }
}

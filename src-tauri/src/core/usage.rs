//! Plan usage percentages, read from the history file Claude Desktop leaves in a profile folder.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

const USAGE_FILE: &str = "plan-usage-history.json";
const FIVE_HOUR: &str = "fh";
const SEVEN_DAY: &str = "sd";

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    pub five_hour: Option<u8>,
    pub seven_day: Option<u8>,
    pub sampled_at: i64,
}

#[derive(Deserialize)]
struct History {
    version: u32,
    samples: Vec<Sample>,
}

/// Version 2 nests the percentages under `u`, version 1 hangs them off the sample itself. Both
/// shapes land here and `version` picks which half to read, so the keys are named exactly once.
#[derive(Deserialize)]
struct Sample {
    t: i64,
    #[serde(default)]
    u: HashMap<String, Value>,
    #[serde(flatten)]
    legacy: HashMap<String, Value>,
}

/// None when the profile has no readable usage history. Claude Desktop writes this file only
/// while a profile runs, so every failure is ordinary and none of them is worth an error: the
/// numbers are decoration on a profile list that has to render regardless.
pub fn read(profile_dir: &Path) -> Option<Usage> {
    let bytes = fs::read(profile_dir.join(USAGE_FILE)).ok()?;
    let history: History = serde_json::from_slice(&bytes).ok()?;
    let sample = history.samples.into_iter().max_by_key(|s| s.t)?;

    let percentages = match history.version {
        1 => &sample.legacy,
        2 => &sample.u,
        _ => return None,
    };

    Some(Usage {
        five_hour: percent(percentages.get(FIVE_HOUR)),
        seven_day: percent(percentages.get(SEVEN_DAY)),
        sampled_at: sample.t,
    })
}

/// An absent, null, or nonsensical entry is "not reported", never zero.
fn percent(value: Option<&Value>) -> Option<u8> {
    u8::try_from(value?.as_u64()?).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile_dir(history: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(USAGE_FILE), history).unwrap();
        dir
    }

    #[test]
    fn the_newest_sample_wins_however_the_file_is_ordered() {
        let dir = profile_dir(
            r#"{"version":2,"samples":[
                {"t":200,"org":null,"u":{"fh":21,"sd":4}},
                {"t":100,"org":null,"u":{"fh":90,"sd":80}}
            ]}"#,
        );
        let usage = read(dir.path()).unwrap();
        assert_eq!(usage.five_hour, Some(21));
        assert_eq!(usage.seven_day, Some(4));
        assert_eq!(usage.sampled_at, 200);
    }

    #[test]
    fn a_limit_the_api_did_not_report_is_none_and_not_zero() {
        let dir = profile_dir(r#"{"version":2,"samples":[{"t":1,"u":{"sd":4,"so":9}}]}"#);
        let usage = read(dir.path()).unwrap();
        assert_eq!(usage.five_hour, None);
        assert_eq!(usage.seven_day, Some(4));
    }

    #[test]
    fn a_version_1_file_reads_its_percentages_off_the_sample() {
        let dir = profile_dir(r#"{"version":1,"samples":[{"t":7,"fh":21,"sd":null}]}"#);
        let usage = read(dir.path()).unwrap();
        assert_eq!(usage.five_hour, Some(21));
        assert_eq!(usage.seven_day, None);
        assert_eq!(usage.sampled_at, 7);
    }

    #[test]
    fn a_corrupt_file_is_no_usage() {
        let dir = profile_dir(r#"{"version":2,"samples":[{"t":"#);
        assert!(read(dir.path()).is_none());
    }

    #[test]
    fn a_missing_file_is_no_usage() {
        let dir = tempfile::tempdir().unwrap();
        assert!(read(dir.path()).is_none());
    }

    #[test]
    fn a_version_this_build_does_not_understand_is_no_usage() {
        let dir = profile_dir(r#"{"version":3,"samples":[{"t":1,"u":{"fh":21}}]}"#);
        assert!(read(dir.path()).is_none());
    }

    #[test]
    fn a_profile_that_has_never_sampled_is_no_usage() {
        let dir = profile_dir(r#"{"version":2,"samples":[]}"#);
        assert!(read(dir.path()).is_none());
    }

    #[test]
    fn the_serialized_keys_are_camel_case() {
        let usage = Usage {
            five_hour: Some(21),
            seven_day: None,
            sampled_at: 5,
        };
        let json = serde_json::to_string(&usage).unwrap();
        assert_eq!(json, r#"{"fiveHour":21,"sevenDay":null,"sampledAt":5}"#);
    }
}

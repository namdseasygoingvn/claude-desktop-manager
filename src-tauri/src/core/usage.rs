//! Plan usage, preferring Claude Desktop's cached `/usage` response and falling back to the
//! history file it leaves in a profile folder.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::usage_cache::{self, CachedUsage, Miss};

const USAGE_FILE: &str = "plan-usage-history.json";
const FIVE_HOUR: &str = "fh";
const SEVEN_DAY: &str = "sd";

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    pub five_hour: Option<u8>,
    pub seven_day: Option<u8>,
    pub seven_day_scoped: Option<u8>,
    pub sampled_at: i64,
    pub five_hour_resets_at: Option<i64>,
    pub seven_day_resets_at: Option<i64>,
    pub seven_day_scoped_resets_at: Option<i64>,
    pub seven_day_scoped_model: Option<String>,
    pub source: UsageSource,
}

/// Which of the two on-disk records the figures came from, and so whether reset times were
/// available at all: the history file records the plain percentages without their clocks, and
/// never the per-model scoped limit at all.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum UsageSource {
    Cache,
    NoCacheEntry,
    CacheUnreadable,
}

impl From<Miss> for UsageSource {
    fn from(miss: Miss) -> Self {
        match miss {
            Miss::NoEntry => UsageSource::NoCacheEntry,
            Miss::Unreadable => UsageSource::CacheUnreadable,
        }
    }
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

/// None when neither record is readable. Claude Desktop writes both only while a profile runs,
/// so every failure is ordinary and none of them is worth an error: the numbers are decoration
/// on a profile list that has to render regardless.
pub fn read(profile_dir: &Path) -> Option<Usage> {
    match usage_cache::read(profile_dir) {
        Ok(cached) => Some(from_cache(cached)),
        Err(miss) => from_history(profile_dir, miss.into()),
    }
}

fn from_cache(cached: CachedUsage) -> Usage {
    let (seven_day_scoped, seven_day_scoped_resets_at, seven_day_scoped_model) =
        match cached.seven_day_scoped {
            Some(scoped) => (Some(scoped.percent), scoped.resets_at, Some(scoped.model)),
            None => (None, None, None),
        };
    Usage {
        five_hour: cached.five_hour.percent,
        seven_day: cached.seven_day.percent,
        seven_day_scoped,
        sampled_at: cached.sampled_at,
        five_hour_resets_at: cached.five_hour.resets_at,
        seven_day_resets_at: cached.seven_day.resets_at,
        seven_day_scoped_resets_at,
        seven_day_scoped_model,
        source: UsageSource::Cache,
    }
}

fn from_history(profile_dir: &Path, source: UsageSource) -> Option<Usage> {
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
        seven_day_scoped: None,
        sampled_at: sample.t,
        five_hour_resets_at: None,
        seven_day_resets_at: None,
        seven_day_scoped_resets_at: None,
        seven_day_scoped_model: None,
        source,
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
            seven_day_scoped: Some(9),
            sampled_at: 5,
            five_hour_resets_at: Some(7),
            seven_day_resets_at: None,
            seven_day_scoped_resets_at: Some(11),
            seven_day_scoped_model: Some("Fable".to_string()),
            source: UsageSource::Cache,
        };
        let json = serde_json::to_string(&usage).unwrap();
        assert_eq!(
            json,
            r#"{"fiveHour":21,"sevenDay":null,"sevenDayScoped":9,"sampledAt":5,"fiveHourResetsAt":7,"sevenDayResetsAt":null,"sevenDayScopedResetsAt":11,"sevenDayScopedModel":"Fable","source":"cache"}"#
        );
    }

    #[test]
    fn the_serialized_sources_are_camel_case() {
        let sources = [
            UsageSource::Cache,
            UsageSource::NoCacheEntry,
            UsageSource::CacheUnreadable,
        ];
        let json = serde_json::to_string(&sources).unwrap();
        assert_eq!(json, r#"["cache","noCacheEntry","cacheUnreadable"]"#);
    }

    #[test]
    fn a_profile_with_no_cache_falls_back_to_its_history() {
        let dir = profile_dir(r#"{"version":2,"samples":[{"t":1,"u":{"fh":21,"sd":4}}]}"#);
        let usage = read(dir.path()).unwrap();
        assert_eq!(usage.source, UsageSource::NoCacheEntry);
        assert_eq!(usage.five_hour_resets_at, None);
        assert_eq!(usage.seven_day_resets_at, None);
        assert_eq!(usage.seven_day_scoped, None);
        assert_eq!(usage.seven_day_scoped_resets_at, None);
        assert_eq!(usage.seven_day_scoped_model, None);
    }

    #[test]
    fn a_cached_response_beats_the_history_file() {
        let dir = profile_dir(r#"{"version":2,"samples":[{"t":1,"u":{"fh":90,"sd":90}}]}"#);
        usage_cache::fixture::entry(
            dir.path(),
            "a",
            br#"{"five_hour":{"utilization":8.0,"resets_at":"2026-08-08T14:09:59.822762+00:00"},
                 "seven_day":{"utilization":27.0,"resets_at":null}}"#,
        );
        let usage = read(dir.path()).unwrap();
        assert_eq!(usage.source, UsageSource::Cache);
        assert_eq!(usage.five_hour, Some(8));
        assert_eq!(usage.seven_day, Some(27));
        assert_eq!(usage.five_hour_resets_at, Some(1_786_198_199_822));
        assert_eq!(usage.seven_day_resets_at, None);
    }

    #[test]
    fn a_weekly_scoped_limit_in_the_cache_surfaces_its_model_and_percent() {
        let dir = profile_dir(r#"{"version":2,"samples":[{"t":1,"u":{"fh":90,"sd":90}}]}"#);
        usage_cache::fixture::entry(
            dir.path(),
            "a",
            br#"{"five_hour":{"utilization":8.0,"resets_at":"2026-08-08T14:09:59.822762+00:00"},
                 "seven_day":{"utilization":27.0,"resets_at":null},
                 "limits":[{"kind":"weekly_scoped","group":"weekly","percent":9,"severity":"normal","resets_at":"2026-08-19T11:59:59.750842+00:00","scope":{"model":{"id":null,"display_name":"Fable"},"surface":null},"is_active":false}]}"#,
        );
        let usage = read(dir.path()).unwrap();
        assert_eq!(usage.seven_day_scoped, Some(9));
        assert_eq!(usage.seven_day_scoped_model, Some("Fable".to_string()));
        assert!(usage.seven_day_scoped_resets_at.is_some());
    }

    #[test]
    fn a_cache_entry_that_will_not_decode_falls_back_to_the_history() {
        let dir = profile_dir(r#"{"version":2,"samples":[{"t":1,"u":{"fh":21,"sd":4}}]}"#);
        usage_cache::fixture::entry(dir.path(), "a", &[0x00, 0x01, 0x02, 0x03]);
        let usage = read(dir.path()).unwrap();
        assert_eq!(usage.source, UsageSource::CacheUnreadable);
        assert_eq!(usage.five_hour, Some(21));
    }

    #[test]
    fn neither_record_readable_is_no_usage() {
        let dir = tempfile::tempdir().unwrap();
        usage_cache::fixture::entry(dir.path(), "a", &[0x00, 0x01, 0x02, 0x03]);
        assert!(read(dir.path()).is_none());
    }
}

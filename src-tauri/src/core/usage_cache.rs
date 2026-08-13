//! The `/usage` response sitting in Claude Desktop's own HTTP cache, the one place on disk that
//! still carries the reset clocks `plan-usage-history.json` throws away.

use std::fs::{self, File};
use std::io::Read as _;
use std::ops::{Range, RangeInclusive};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::Deserialize;

const CACHE_DIR: [&str; 2] = ["Cache", "Cache_Data"];
const USAGE_PATH: &str = "/usage";

const MAGIC: u64 = 0xfcfb_6d1b_a772_5c30;
const VERSION: u32 = 5;
const KEY_LEN_AT: usize = 12;
const KEY_AT: usize = 24;
const EOF_LEN: usize = 20;
/// A length past this is garbage rather than a URL, and allocating on it is how one corrupt
/// file would turn into an out-of-memory abort.
const MAX_KEY_LEN: usize = 8192;

#[derive(Debug, Default)]
pub struct Limit {
    pub percent: Option<u8>,
    pub resets_at: Option<i64>,
}

#[derive(Debug)]
pub struct ScopedLimit {
    pub model: String,
    pub percent: u8,
    pub resets_at: Option<i64>,
}

#[derive(Debug)]
pub struct CachedUsage {
    pub five_hour: Limit,
    pub seven_day: Limit,
    pub seven_day_scoped: Option<ScopedLimit>,
    pub sampled_at: i64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Miss {
    NoEntry,
    Unreadable,
}

struct Entry {
    path: PathBuf,
    modified: i64,
}

/// The newest cached response the profile has, if any of them still decodes.
pub fn read(profile_dir: &Path) -> Result<CachedUsage, Miss> {
    let mut entries = usage_entries(&cache_dir(profile_dir));
    if entries.is_empty() {
        return Err(Miss::NoEntry);
    }
    entries.sort_by_key(|entry| std::cmp::Reverse(entry.modified));
    entries
        .iter()
        .find_map(|entry| Some(cached(decode(&entry.path)?, entry.modified)))
        .ok_or(Miss::Unreadable)
}

fn cache_dir(profile_dir: &Path) -> PathBuf {
    CACHE_DIR
        .iter()
        .fold(profile_dir.to_path_buf(), |path, part| path.join(part))
}

fn usage_entries(dir: &Path) -> Vec<Entry> {
    let Ok(listing) = fs::read_dir(dir) else {
        return Vec::new();
    };
    listing
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| key_of(path).is_some_and(|key| is_usage_key(&key)))
        .filter_map(|path| {
            let modified = modified_ms(&path)?;
            Some(Entry { path, modified })
        })
        .collect()
}

/// Read only far enough to name the request: the cache holds hundreds of entries and all but
/// one of them is somebody else's response.
fn key_of(path: &Path) -> Option<String> {
    let mut file = File::open(path).ok()?;
    let mut header = [0u8; KEY_AT];
    file.read_exact(&mut header).ok()?;

    let key_len = u32_at(&header, KEY_LEN_AT)? as usize;
    if key_len > MAX_KEY_LEN {
        return None;
    }
    let mut key = vec![0u8; key_len];
    file.read_exact(&mut key).ok()?;
    String::from_utf8(key).ok()
}

/// Keys look like `1/0/https://claude.ai/api/organizations/<org>/usage`, sometimes with a query.
fn is_usage_key(key: &str) -> bool {
    key.split('?').next().unwrap_or(key).ends_with(USAGE_PATH)
}

fn modified_ms(path: &Path) -> Option<i64> {
    let modified = fs::metadata(path).ok()?.modified().ok()?;
    i64::try_from(modified.duration_since(UNIX_EPOCH).ok()?.as_millis()).ok()
}

fn decode(path: &Path) -> Option<Response> {
    let data = fs::read(path).ok()?;
    if u64_at(&data, 0)? != MAGIC || u32_at(&data, 8)? != VERSION {
        return None;
    }
    let key_len = u32_at(&data, KEY_LEN_AT)? as usize;
    payload(data.get(body_range(&data, key_len)?)?)
}

/// The body sits between the key and the trailing stream-0 record, whose size is the last thing
/// in the file. Every offset here is derived from lengths a truncated file can make nonsense of.
fn body_range(data: &[u8], key_len: usize) -> Option<Range<usize>> {
    let stream0_len = u32_at(data, data.len().checked_sub(8)?)? as usize;
    let start = KEY_AT.checked_add(key_len)?;
    let end = data
        .len()
        .checked_sub(EOF_LEN)?
        .checked_sub(stream0_len)?
        .checked_sub(EOF_LEN)?;
    (start <= end).then_some(start..end)
}

/// Whatever content-encoding the response carried. zstd is what Claude negotiates today; gzip
/// and identity cost nothing to try and spare us a dead reader the day that changes.
fn payload(body: &[u8]) -> Option<Response> {
    [unzstd(body), ungzip(body), Some(body.to_vec())]
        .into_iter()
        .flatten()
        .find_map(|json| serde_json::from_slice(&json).ok())
}

/// Single-frame, because the stored bytes run a little past the response: reading on would only
/// find the cache's own trailing record and call the whole thing corrupt.
fn unzstd(body: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    zstd::Decoder::new(body)
        .ok()?
        .single_frame()
        .read_to_end(&mut out)
        .ok()?;
    Some(out)
}

fn ungzip(body: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    flate2::read::GzDecoder::new(body)
        .read_to_end(&mut out)
        .ok()?;
    Some(out)
}

fn cached(response: Response, modified: i64) -> CachedUsage {
    CachedUsage {
        five_hour: response.five_hour.into(),
        seven_day: response.seven_day.into(),
        seven_day_scoped: scoped_limit(&response.limits),
        sampled_at: modified,
    }
}

#[derive(Deserialize)]
struct Response {
    five_hour: Option<Window>,
    seven_day: Option<Window>,
    #[serde(default)]
    limits: Vec<LimitEntry>,
}

#[derive(Deserialize)]
struct Window {
    utilization: Option<f64>,
    resets_at: Option<String>,
}

#[derive(Deserialize)]
struct LimitEntry {
    kind: String,
    percent: Option<i64>,
    resets_at: Option<String>,
    scope: Option<Scope>,
}

#[derive(Deserialize)]
struct Scope {
    model: Option<ModelScope>,
}

#[derive(Deserialize)]
struct ModelScope {
    display_name: Option<String>,
}

/// `weekly_scoped` is the per-model limit (Fable's weekly cap, say) layered on top of the
/// account-wide `weekly_all`; the first one with a name and an in-range percent wins, and one
/// missing either is as good as not in the array.
fn scoped_limit(limits: &[LimitEntry]) -> Option<ScopedLimit> {
    limits.iter().find_map(|limit| {
        if limit.kind != "weekly_scoped" {
            return None;
        }
        let model = limit.scope.as_ref()?.model.as_ref()?.display_name.clone()?;
        let percent = u8::try_from(limit.percent?).ok()?;
        Some(ScopedLimit {
            model,
            percent,
            resets_at: limit.resets_at.as_deref().and_then(epoch_ms),
        })
    })
}

impl From<Option<Window>> for Limit {
    fn from(window: Option<Window>) -> Self {
        let Some(window) = window else {
            return Limit::default();
        };
        Limit {
            percent: window.utilization.map(percent),
            resets_at: window.resets_at.as_deref().and_then(epoch_ms),
        }
    }
}

fn percent(utilization: f64) -> u8 {
    utilization.round().clamp(0.0, u8::MAX.into()) as u8
}

/// RFC3339 as the API writes it, microseconds and offset included. A shape this does not
/// recognise leaves the reset unknown rather than guessed at.
fn epoch_ms(stamp: &str) -> Option<i64> {
    let (date, time) = stamp.split_once('T')?;
    let mut ymd = date.splitn(3, '-');
    let year = field(ymd.next(), 1..=9_999)?;
    let month = field(ymd.next(), 1..=12)?;
    let day = field(ymd.next(), 1..=31)?;

    let (clock, offset) = split_offset(time)?;
    let (clock, fraction) = clock.split_once('.').unwrap_or((clock, ""));
    let mut hms = clock.splitn(3, ':');
    let hour = field(hms.next(), 0..=23)?;
    let minute = field(hms.next(), 0..=59)?;
    let second = field(hms.next(), 0..=60)?;

    let seconds = days_from_civil(year, month, day) * 86_400 + hour * 3_600 + minute * 60 + second;
    Some((seconds - offset) * 1_000 + millis(fraction))
}

/// Out-of-range is as good as unparseable here, and it keeps the arithmetic below in bounds.
fn field(text: Option<&str>, allowed: RangeInclusive<i64>) -> Option<i64> {
    let value: i64 = text?.parse().ok()?;
    allowed.contains(&value).then_some(value)
}

fn split_offset(time: &str) -> Option<(&str, i64)> {
    if let Some(clock) = time.strip_suffix('Z') {
        return Some((clock, 0));
    }
    let (clock, zone) = time.split_at(time.rfind(['+', '-'])?);
    let (hours, minutes) = zone.get(1..)?.split_once(':')?;
    let seconds = field(Some(hours), 0..=23)? * 3_600 + field(Some(minutes), 0..=59)? * 60;
    Some((clock, if zone.starts_with('-') { -seconds } else { seconds }))
}

fn millis(fraction: &str) -> i64 {
    let digits: String = fraction.chars().chain("000".chars()).take(3).collect();
    digits.parse().unwrap_or(0)
}

/// Hinnant's `days_from_civil`: a proleptic Gregorian date to days since the Unix epoch.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let yoe = year - era * 400;
    let mp = if month > 2 { month - 3 } else { month + 9 };
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn u32_at(data: &[u8], at: usize) -> Option<u32> {
    let field = data.get(at..at.checked_add(4)?)?;
    Some(u32::from_le_bytes(field.try_into().ok()?))
}

fn u64_at(data: &[u8], at: usize) -> Option<u64> {
    let field = data.get(at..at.checked_add(8)?)?;
    Some(u64::from_le_bytes(field.try_into().ok()?))
}

#[cfg(test)]
pub(crate) mod fixture {
    use super::*;

    pub(crate) const KEY: &str = "1/0/https://claude.ai/api/organizations/org-1/usage";

    pub(crate) fn record(magic: u64, version: u32, key: &str, body: &[u8]) -> Vec<u8> {
        let stream0 = [0u8; 8];
        let mut eof0 = vec![0u8; EOF_LEN];
        eof0[EOF_LEN - 8..EOF_LEN - 4].copy_from_slice(&(stream0.len() as u32).to_le_bytes());

        let mut out = Vec::new();
        out.extend_from_slice(&magic.to_le_bytes());
        out.extend_from_slice(&version.to_le_bytes());
        out.extend_from_slice(&(key.len() as u32).to_le_bytes());
        out.extend_from_slice(&[0u8; 8]);
        out.extend_from_slice(key.as_bytes());
        out.extend_from_slice(body);
        out.extend_from_slice(&[0u8; EOF_LEN]);
        out.extend_from_slice(&stream0);
        out.extend_from_slice(&eof0);
        out
    }

    pub(crate) fn write(profile_dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
        let dir = cache_dir(profile_dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("{name}_0"));
        fs::write(&path, bytes).unwrap();
        path
    }

    pub(crate) fn entry(profile_dir: &Path, name: &str, body: &[u8]) -> PathBuf {
        write(profile_dir, name, &record(MAGIC, VERSION, KEY, body))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RESPONSE: &[u8] = br#"{"organization_id":"org-1",
        "five_hour":{"utilization":8.0,"resets_at":"2026-08-08T14:09:59.822762+00:00"},
        "seven_day":{"utilization":27.4,"resets_at":"2026-08-12T11:59:59.822785+00:00"}}"#;

    fn profile_dir() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn a_cached_response_yields_its_percentages_and_reset_times() {
        let dir = profile_dir();
        fixture::entry(dir.path(), "a", RESPONSE);
        let usage = read(dir.path()).unwrap();
        assert_eq!(usage.five_hour.percent, Some(8));
        assert_eq!(usage.five_hour.resets_at, Some(1_786_198_199_822));
        assert_eq!(usage.seven_day.percent, Some(27));
        assert_eq!(usage.seven_day.resets_at, Some(1_786_535_999_822));
    }

    #[test]
    fn the_sample_time_is_the_entry_file_itself() {
        let dir = profile_dir();
        let path = fixture::entry(dir.path(), "a", RESPONSE);
        let usage = read(dir.path()).unwrap();
        assert_eq!(usage.sampled_at, modified_ms(&path).unwrap());
    }

    #[test]
    fn a_zstd_body_decodes_like_a_plain_one() {
        let dir = profile_dir();
        fixture::entry(dir.path(), "a", &zstd::encode_all(RESPONSE, 0).unwrap());
        assert_eq!(read(dir.path()).unwrap().five_hour.percent, Some(8));
    }

    #[test]
    fn a_limit_the_api_reported_without_a_reset_keeps_its_percentage() {
        let dir = profile_dir();
        fixture::entry(
            dir.path(),
            "a",
            br#"{"five_hour":{"utilization":0.0,"resets_at":null},"seven_day":null}"#,
        );
        let usage = read(dir.path()).unwrap();
        assert_eq!(usage.five_hour.percent, Some(0));
        assert_eq!(usage.five_hour.resets_at, None);
        assert_eq!(usage.seven_day.percent, None);
    }

    #[test]
    fn a_body_without_limits_has_no_scoped_limit() {
        let dir = profile_dir();
        fixture::entry(dir.path(), "a", RESPONSE);
        assert!(read(dir.path()).unwrap().seven_day_scoped.is_none());
    }

    #[test]
    fn a_weekly_scoped_limit_carries_its_model_and_reset() {
        const RESPONSE: &[u8] = br#"{"five_hour":{"utilization":44.0,"resets_at":"2026-08-13T06:50:00.750655+00:00"},
            "seven_day":{"utilization":6.0,"resets_at":"2026-08-19T12:00:00.750674+00:00"},
            "limits":[
             {"kind":"session","group":"session","percent":44,"severity":"normal","resets_at":"2026-08-13T06:50:00.750655+00:00","scope":null,"is_active":true},
             {"kind":"weekly_all","group":"weekly","percent":6,"severity":"normal","resets_at":"2026-08-19T12:00:00.750674+00:00","scope":null,"is_active":false},
             {"kind":"weekly_scoped","group":"weekly","percent":9,"severity":"normal","resets_at":"2026-08-19T11:59:59.750842+00:00","scope":{"model":{"id":null,"display_name":"Fable"},"surface":null},"is_active":false}
            ]}"#;
        let dir = profile_dir();
        fixture::entry(dir.path(), "a", RESPONSE);
        let scoped = read(dir.path()).unwrap().seven_day_scoped.unwrap();
        assert_eq!(scoped.model, "Fable");
        assert_eq!(scoped.percent, 9);
        assert_eq!(scoped.resets_at, Some(1_787_140_799_750));
    }

    #[test]
    fn a_weekly_scoped_limit_missing_its_name_is_none() {
        let dir = profile_dir();
        fixture::entry(
            dir.path(),
            "a",
            br#"{"five_hour":null,"seven_day":null,
                "limits":[{"kind":"weekly_scoped","percent":9,"resets_at":null,"scope":null}]}"#,
        );
        assert!(read(dir.path()).unwrap().seven_day_scoped.is_none());

        let dir = profile_dir();
        fixture::entry(
            dir.path(),
            "a",
            br#"{"five_hour":null,"seven_day":null,
                "limits":[{"kind":"weekly_scoped","percent":9,"resets_at":null,
                    "scope":{"model":{"id":null,"display_name":null}}}]}"#,
        );
        assert!(read(dir.path()).unwrap().seven_day_scoped.is_none());
    }

    #[test]
    fn a_cache_version_this_build_does_not_understand_is_unreadable() {
        let dir = profile_dir();
        let bytes = fixture::record(MAGIC, VERSION + 1, fixture::KEY, RESPONSE);
        fixture::write(dir.path(), "a", &bytes);
        assert_eq!(read(dir.path()).unwrap_err(), Miss::Unreadable);
    }

    #[test]
    fn a_file_without_the_simple_cache_magic_is_unreadable() {
        let dir = profile_dir();
        let bytes = fixture::record(MAGIC ^ 1, VERSION, fixture::KEY, RESPONSE);
        fixture::write(dir.path(), "a", &bytes);
        assert_eq!(read(dir.path()).unwrap_err(), Miss::Unreadable);
    }

    #[test]
    fn an_entry_cut_short_of_its_trailing_records_is_unreadable() {
        let dir = profile_dir();
        let mut bytes = fixture::record(MAGIC, VERSION, fixture::KEY, RESPONSE);
        bytes.truncate(KEY_AT + fixture::KEY.len() + 4);
        fixture::write(dir.path(), "a", &bytes);
        assert_eq!(read(dir.path()).unwrap_err(), Miss::Unreadable);
    }

    #[test]
    fn a_file_too_short_to_hold_a_key_is_no_entry() {
        let dir = profile_dir();
        fixture::write(dir.path(), "a", &[0u8; 10]);
        assert_eq!(read(dir.path()).unwrap_err(), Miss::NoEntry);
    }

    #[test]
    fn a_body_no_decoder_understands_is_unreadable() {
        let dir = profile_dir();
        fixture::entry(dir.path(), "a", &[0x00, 0x01, 0x02, 0x03]);
        assert_eq!(read(dir.path()).unwrap_err(), Miss::Unreadable);
    }

    #[test]
    fn a_cache_holding_other_requests_is_no_entry() {
        let dir = profile_dir();
        let key = "1/0/https://claude.ai/api/organizations/org-1/projects";
        fixture::write(dir.path(), "a", &fixture::record(MAGIC, VERSION, key, RESPONSE));
        assert_eq!(read(dir.path()).unwrap_err(), Miss::NoEntry);
    }

    #[test]
    fn a_profile_with_no_cache_at_all_is_no_entry() {
        assert_eq!(read(profile_dir().path()).unwrap_err(), Miss::NoEntry);
    }

    #[test]
    fn the_query_string_on_a_usage_key_does_not_hide_it() {
        assert!(is_usage_key(&format!("{}?skip_spend=1", fixture::KEY)));
    }

    #[test]
    fn the_newest_entry_wins() {
        let dir = profile_dir();
        let older = br#"{"five_hour":{"utilization":90.0,"resets_at":null},"seven_day":null}"#;
        let old = fixture::entry(dir.path(), "old", older);
        fixture::entry(dir.path(), "new", RESPONSE);
        let long_ago = std::time::SystemTime::now() - std::time::Duration::from_secs(3_600);
        File::options()
            .write(true)
            .open(&old)
            .unwrap()
            .set_modified(long_ago)
            .unwrap();
        assert_eq!(read(dir.path()).unwrap().five_hour.percent, Some(8));
    }

    #[test]
    fn a_timestamp_with_an_offset_lands_in_utc() {
        let utc = 1_786_198_199_000;
        assert_eq!(epoch_ms("2026-08-08T14:09:59.822762+00:00"), Some(utc + 822));
        assert_eq!(epoch_ms("2026-08-08T14:09:59Z"), Some(utc));
        assert_eq!(epoch_ms("2026-08-08T09:09:59-05:00"), Some(utc));
    }

    #[test]
    fn a_timestamp_in_a_shape_we_do_not_read_is_none() {
        assert_eq!(epoch_ms("2026-08-08"), None);
        assert_eq!(epoch_ms(""), None);
        assert_eq!(epoch_ms("later"), None);
        assert_eq!(epoch_ms("999999999999999-08-08T14:09:59Z"), None);
        assert_eq!(epoch_ms("2026-08-08T999999999999999:09:59Z"), None);
    }
}

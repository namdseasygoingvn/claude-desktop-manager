//! When Claude Desktop is closed, its cached `/usage` response freezes at the last sample.
//! If a quota window has since elapsed — the reset timestamp is in the past — the limit did
//! reset; project it forward rather than keep reporting a percentage that is no longer spent.

use std::time::{SystemTime, UNIX_EPOCH};

use super::usage::Usage;

const FIVE_HOURS_MS: i64 = 5 * 60 * 60 * 1_000;
const SEVEN_DAYS_MS: i64 = 7 * 24 * 60 * 60 * 1_000;

pub fn apply(usage: &mut Usage) {
    let Some(now) = now_ms() else {
        return;
    };
    roll(
        &mut usage.five_hour,
        &mut usage.five_hour_resets_at,
        FIVE_HOURS_MS,
        now,
    );
    roll(
        &mut usage.seven_day,
        &mut usage.seven_day_resets_at,
        SEVEN_DAYS_MS,
        now,
    );
    roll(
        &mut usage.seven_day_scoped,
        &mut usage.seven_day_scoped_resets_at,
        SEVEN_DAYS_MS,
        now,
    );
}

fn roll(percent: &mut Option<u8>, resets_at: &mut Option<i64>, window: i64, now: i64) {
    let (Some(_), Some(previous)) = (*percent, *resets_at) else {
        return;
    };
    if previous > now {
        return;
    }
    let Some(next) = next_boundary(previous, window, now) else {
        return;
    };
    *percent = Some(0);
    *resets_at = Some(next);
}

/// Snaps to whole windows so a reset that landed on the hour keeps landing on the hour
/// however long the profile sat closed.
fn next_boundary(previous: i64, window: i64, now: i64) -> Option<i64> {
    let elapsed = now.checked_sub(previous)?;
    previous.checked_add(window.checked_mul(elapsed / window + 1)?)
}

fn now_ms() -> Option<i64> {
    i64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .ok()?
            .as_millis(),
    )
    .ok()
}

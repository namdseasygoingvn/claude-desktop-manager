use std::sync::{Mutex, OnceLock};

const CAPACITY: usize = 400;

static LINES: OnceLock<Mutex<Vec<String>>> = OnceLock::new();

fn lines() -> &'static Mutex<Vec<String>> {
    LINES.get_or_init(|| Mutex::new(Vec::new()))
}

/// A poisoned lock costs at most a garbled debug line, so recover rather than propagate.
fn locked() -> std::sync::MutexGuard<'static, Vec<String>> {
    lines().lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub fn snapshot() -> Vec<String> {
    locked().clone()
}

pub fn clear() {
    locked().clear();
}

/// Mirrors every `log::` record into the ring buffer, so `get_logs` sees the same events the
/// app already reports rather than needing its own instrumentation at each call site. The
/// plugin has already stamped and labelled the record, so the line is stored verbatim.
pub struct Sink;

impl log::Log for Sink {
    fn enabled(&self, _: &log::Metadata) -> bool {
        true
    }

    fn log(&self, record: &log::Record) {
        let mut lines = locked();
        lines.push(record.args().to_string());
        if lines.len() > CAPACITY {
            let overflow = lines.len() - CAPACITY;
            lines.drain(..overflow);
        }
    }

    fn flush(&self) {}
}

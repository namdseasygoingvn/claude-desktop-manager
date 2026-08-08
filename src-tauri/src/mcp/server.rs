//! The one listener that may be up at a time, and the liveness facts the General tab reads
//! back. Everything here is process-global because the port is: two listeners cannot share it.

use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::thread::JoinHandle;
use std::time::Instant;

use super::http::{self, Endpoint};

struct Running {
    port: u16,
    tools: usize,
    started: Instant,
    shutdown: Arc<AtomicBool>,
    thread: JoinHandle<()>,
}

/// What the server is doing right now, for the status readout.
pub struct Snapshot {
    pub port: u16,
    pub tools: usize,
    pub uptime_seconds: u64,
    pub requests: u64,
}

static RUNNING: OnceLock<Mutex<Option<Running>>> = OnceLock::new();
static LAST_ERROR: OnceLock<Mutex<Option<String>>> = OnceLock::new();
static REQUESTS: AtomicU64 = AtomicU64::new(0);

/// A poisoned lock costs at most a stale status line, so recover rather than propagate — the
/// same bargain `logbuf` makes.
fn running() -> MutexGuard<'static, Option<Running>> {
    RUNNING
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn error() -> MutexGuard<'static, Option<String>> {
    LAST_ERROR
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Bind `port` and serve on it, replacing whatever was up before. The endpoint is built from
/// the port that was actually claimed, not the one asked for, so `get_app_info` reports the
/// URL a client can really reach when `port` was 0.
pub fn start(port: u16, endpoint: impl FnOnce(u16) -> Endpoint) -> Result<u16, String> {
    stop();

    let (listener, bound) = match http::bind(port) {
        Ok(bound) => bound,
        Err(err) => {
            let detail = err.to_string();
            *error() = Some(detail.clone());
            return Err(detail);
        }
    };

    let built = endpoint(bound);
    let tools = built.tools.len();
    let shutdown = Arc::new(AtomicBool::new(false));
    let thread = http::serve(listener, built, Arc::clone(&shutdown));

    REQUESTS.store(0, Ordering::Relaxed);
    *error() = None;
    *running() = Some(Running {
        port: bound,
        tools,
        started: Instant::now(),
        shutdown,
        thread,
    });
    Ok(bound)
}

/// Release the port and wait for the accept loop to actually let go of it, so the very next
/// `start` — a port edit is exactly that — cannot race its own predecessor for the socket.
pub fn stop() {
    // First, and unconditionally: a bind that failed says nothing about a server the user has
    // since asked to be off, and the status line would otherwise keep reporting it.
    *error() = None;

    let Some(running) = running().take() else {
        return;
    };
    running.shutdown.store(true, Ordering::Relaxed);
    // `incoming()` is parked in accept(); only a connection wakes it up to see the flag.
    let _ = TcpStream::connect(("127.0.0.1", running.port));
    let _ = running.thread.join();
}

pub fn snapshot() -> Option<Snapshot> {
    running().as_ref().map(|running| Snapshot {
        port: running.port,
        tools: running.tools,
        uptime_seconds: running.started.elapsed().as_secs(),
        requests: REQUESTS.load(Ordering::Relaxed),
    })
}

/// The bind failure the user has to see, kept until a start succeeds or the server is turned off.
pub fn last_error() -> Option<String> {
    error().clone()
}

pub fn record_request() {
    REQUESTS.fetch_add(1, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::super::rpc::{ServerInfo, Tool};
    use super::*;
    use serde_json::json;

    fn endpoint(_: u16) -> Endpoint {
        Endpoint {
            info: ServerInfo {
                name: "cdm".to_string(),
                version: "test".to_string(),
            },
            tools: vec![Tool {
                name: "echo",
                description: "Echo the arguments back.",
                input_schema: json!({"type": "object"}),
                handler: Box::new(|args| Ok(args.clone())),
            }],
            path: "/mcp",
        }
    }

    /// One test, not three: the listener is process-global, so parallel cases would each be
    /// looking at whichever server the others had most recently left up.
    #[test]
    fn the_switch_gives_the_port_back_and_says_why_when_it_cannot_take_it() {
        // Off and on again has to work, or a port edit would fail on its own leftovers.
        let first = start(0, endpoint).expect("first bind");
        assert_eq!(snapshot().map(|s| s.port), Some(first));
        assert_eq!(snapshot().map(|s| s.tools), Some(1));

        stop();
        assert!(snapshot().is_none());

        let again = start(first, endpoint).expect("rebind the same port");
        assert_eq!(again, first);
        stop();

        // A refused bind is the one failure the user has to be told about verbatim.
        let squatter = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("squat a port");
        let taken = squatter.local_addr().unwrap().port();

        let err = start(taken, endpoint).expect_err("the port is not free");
        assert!(!err.is_empty());
        assert!(snapshot().is_none());
        assert_eq!(last_error().as_deref(), Some(err.as_str()));

        stop();
        assert!(last_error().is_none());
    }
}

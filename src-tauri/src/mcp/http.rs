use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};

use super::rpc::{self, ServerInfo, Tool};

const MAX_BODY: usize = 1024 * 1024;
const READ_TIMEOUT: Duration = Duration::from_secs(30);

/// One MCP endpoint: the server identity a client sees plus the tools it may call.
pub struct Endpoint {
    pub info: ServerInfo,
    pub tools: Vec<Tool>,
    pub path: &'static str,
}

/// Claim the loopback port. Split from `serve` so the caller knows the real port — which
/// differs from what it asked for when `port` is 0 — before it builds the endpoint.
pub fn bind(port: u16) -> std::io::Result<(TcpListener, u16)> {
    let listener = TcpListener::bind(("127.0.0.1", port))?;
    let bound = listener.local_addr()?.port();
    Ok((listener, bound))
}

/// Serve on a background thread until the process exits.
///
/// Stateless Streamable HTTP: every POST carries one JSON-RPC message and gets one JSON
/// reply. No server→client stream is ever opened, so GET is a 405.
pub fn serve(listener: TcpListener, endpoint: Endpoint) {
    let endpoint = Arc::new(endpoint);
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { break };
            let endpoint = Arc::clone(&endpoint);
            thread::spawn(move || handle(stream, &endpoint));
        }
    });
}

fn handle(mut stream: TcpStream, endpoint: &Endpoint) {
    let _ = stream.set_read_timeout(Some(READ_TIMEOUT));

    let Some(request) = read_request(&mut stream) else {
        respond(&mut stream, 400, Some(&json!({"error": "malformed request"})), &[]);
        return;
    };

    if let Some(origin) = request.headers.get("origin") {
        if !origin_allowed(origin) {
            respond(&mut stream, 403, Some(&json!({"error": "origin not allowed"})), &[]);
            return;
        }
    }

    let path = request.path.split('?').next().unwrap_or("");
    if path != endpoint.path {
        respond(&mut stream, 404, Some(&json!({"error": "not found"})), &[]);
        return;
    }

    match request.method.as_str() {
        "POST" => post(&mut stream, endpoint, &request.body),
        // Stateless: nothing to close, nothing to stream.
        "DELETE" => respond(&mut stream, 200, Some(&json!({"ok": true})), &[]),
        "OPTIONS" => respond(&mut stream, 204, None, &[]),
        _ => respond(
            &mut stream,
            405,
            Some(&json!({"error": "method not allowed"})),
            &[],
        ),
    }
}

fn post(stream: &mut TcpStream, endpoint: &Endpoint, body: &[u8]) {
    let Ok(message) = serde_json::from_slice::<Value>(body) else {
        let parse_error = json!({
            "jsonrpc": "2.0",
            "id": Value::Null,
            "error": {"code": -32700, "message": "parse error"},
        });
        respond(stream, 400, Some(&parse_error), &[]);
        return;
    };

    let reply = rpc::dispatch(&message, &endpoint.tools, &endpoint.info);
    // Through `log` rather than straight into the buffer, so request breadcrumbs are stamped
    // and shaped exactly like every other line `get_logs` returns.
    log::info!(target: "mcp", "{}", rpc::label(&message));

    let headers = if reply.is_initialize {
        vec![("Mcp-Session-Id".to_string(), session_id())]
    } else {
        Vec::new()
    };
    respond(stream, reply.status, reply.body.as_ref(), &headers);
}

struct Request {
    method: String,
    path: String,
    headers: HashMap<String, String>,
    body: Vec<u8>,
}

fn read_request(stream: &mut TcpStream) -> Option<Request> {
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 8192];

    let body_start = loop {
        if let Some(index) = header_end(&buffer) {
            break index;
        }
        let read = stream.read(&mut chunk).ok()?;
        if read == 0 || buffer.len() + read > MAX_BODY {
            return None;
        }
        buffer.extend_from_slice(&chunk[..read]);
    };

    let (method, path, headers) = parse_head(&buffer[..body_start])?;
    let length = headers
        .get("content-length")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    if length > MAX_BODY {
        return None;
    }

    while buffer.len() - body_start < length {
        let read = stream.read(&mut chunk).ok()?;
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..read]);
        if buffer.len() > MAX_BODY {
            return None;
        }
    }

    let end = (body_start + length).min(buffer.len());
    Some(Request {
        method,
        path,
        headers,
        body: buffer[body_start..end].to_vec(),
    })
}

/// Index just past the `\r\n\r\n` that ends the header block.
fn header_end(buffer: &[u8]) -> Option<usize> {
    buffer
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|index| index + 4)
}

fn parse_head(head: &[u8]) -> Option<(String, String, HashMap<String, String>)> {
    let text = std::str::from_utf8(head).ok()?;
    let mut lines = text.split("\r\n");

    let mut request_line = lines.next()?.split_whitespace();
    let method = request_line.next()?.to_uppercase();
    let path = request_line.next()?.to_string();

    let headers = lines
        .filter(|line| !line.is_empty())
        .filter_map(|line| line.split_once(':'))
        .map(|(key, value)| (key.trim().to_lowercase(), value.trim().to_string()))
        .collect();

    Some((method, path, headers))
}

fn respond(stream: &mut TcpStream, status: u16, body: Option<&Value>, extra: &[(String, String)]) {
    let payload = body.map(|value| value.to_string().into_bytes());

    let mut head = format!("HTTP/1.1 {status} {}\r\nConnection: close\r\n", reason(status));
    match &payload {
        Some(bytes) => head.push_str(&format!(
            "Content-Type: application/json\r\nContent-Length: {}\r\n",
            bytes.len()
        )),
        None => head.push_str("Content-Length: 0\r\n"),
    }
    for (key, value) in extra {
        head.push_str(&format!("{key}: {value}\r\n"));
    }
    head.push_str("\r\n");

    let mut out = head.into_bytes();
    if let Some(bytes) = payload {
        out.extend_from_slice(&bytes);
    }
    let _ = stream.write_all(&out);
    let _ = stream.flush();
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        202 => "Accepted",
        204 => "No Content",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        _ => "OK",
    }
}

/// DNS-rebind guard: a browser page on any other origin is refused. A CLI MCP client
/// sends no Origin at all, which is why a missing header is allowed through.
fn origin_allowed(origin: &str) -> bool {
    let after_scheme = origin.split_once("://").map(|(_, rest)| rest).unwrap_or("");
    let authority = after_scheme
        .split(['/', '?', '#'])
        .next()
        .unwrap_or("")
        .trim_end_matches('.');
    let host = match authority.rsplit_once(':') {
        Some((host, port)) if port.chars().all(|c| c.is_ascii_digit()) => host,
        _ => authority,
    };
    matches!(host, "127.0.0.1" | "localhost" | "[::1]" | "::1")
}

/// Opaque per-initialize token. The server is stateless, so this only has to be unique.
fn session_id() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{nanos:x}-{:x}", COUNTER.fetch_add(1, Ordering::Relaxed))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_loopback_origin_is_allowed_and_anything_else_is_not() {
        assert!(origin_allowed("http://127.0.0.1:20204"));
        assert!(origin_allowed("http://localhost"));
        assert!(origin_allowed("http://[::1]:1234/mcp"));
        assert!(!origin_allowed("http://evil.example.com"));
        assert!(!origin_allowed("http://127.0.0.1.evil.com"));
        assert!(!origin_allowed(""));
    }

    #[test]
    fn a_client_can_initialize_list_and_call_over_a_real_socket() {
        let (listener, port) = bind(0).expect("bind loopback");
        serve(
            listener,
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
            },
        );

        let initialize = round_trip(port, r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#);
        assert_eq!(initialize["result"]["serverInfo"]["name"], "cdm");
        assert_eq!(initialize["result"]["protocolVersion"], rpc::PROTOCOL_VERSION);

        let listed = round_trip(port, r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#);
        assert_eq!(listed["result"]["tools"][0]["name"], "echo");

        let called = round_trip(
            port,
            r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"echo","arguments":{"hi":true}}}"#,
        );
        let text = called["result"]["content"][0]["text"]
            .as_str()
            .expect("text content");
        assert!(text.contains("\"hi\": true"), "got {text}");

        let unknown = round_trip(
            port,
            r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"nope"}}"#,
        );
        assert_eq!(unknown["error"]["code"], -32602);
    }

    fn round_trip(port: u16, body: &str) -> Value {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
        let request = format!(
            "POST /mcp HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(request.as_bytes()).expect("write request");

        let mut response = Vec::new();
        stream.read_to_end(&mut response).expect("read response");
        let start = header_end(&response).expect("header terminator");
        serde_json::from_slice(&response[start..]).expect("json body")
    }

    #[test]
    fn the_head_parser_splits_the_request_line_and_lowercases_header_names() {
        let raw = b"POST /mcp HTTP/1.1\r\nContent-Length: 12\r\nOrigin: http://localhost\r\n\r\n";
        let end = header_end(raw).expect("terminator");
        let (method, path, headers) = parse_head(&raw[..end]).expect("head");
        assert_eq!(method, "POST");
        assert_eq!(path, "/mcp");
        assert_eq!(headers.get("content-length").map(String::as_str), Some("12"));
        assert_eq!(
            headers.get("origin").map(String::as_str),
            Some("http://localhost")
        );
    }
}

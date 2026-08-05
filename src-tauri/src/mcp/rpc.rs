use serde_json::{json, Value};

/// The MCP spec revision advertised when a client does not pin one.
pub const PROTOCOL_VERSION: &str = "2025-06-18";

pub type Handler = Box<dyn Fn(&Value) -> Result<Value, String> + Send + Sync>;

pub struct Tool {
    pub name: &'static str,
    pub description: &'static str,
    pub input_schema: Value,
    pub handler: Handler,
}

/// What the transport should send back. `body == None` → an empty response (202/204).
pub struct Reply {
    pub status: u16,
    pub body: Option<Value>,
    pub is_initialize: bool,
}

impl Reply {
    fn new(status: u16, body: Option<Value>) -> Self {
        Self {
            status,
            body,
            is_initialize: false,
        }
    }
}

pub struct ServerInfo {
    pub name: String,
    pub version: String,
}

/// Map one JSON-RPC message to the response the transport should emit. Pure: no sockets,
/// so it can be exercised without a listener.
pub fn dispatch(message: &Value, tools: &[Tool], info: &ServerInfo) -> Reply {
    if message.is_array() {
        return Reply::new(400, Some(json!({"error": "JSON-RPC batching is not supported"})));
    }

    let id = message.get("id").cloned().unwrap_or(Value::Null);
    let method = message.get("method").and_then(Value::as_str);
    let params = message.get("params").cloned().unwrap_or_else(|| json!({}));

    match method {
        Some("initialize") => {
            let requested = params
                .get("protocolVersion")
                .and_then(Value::as_str)
                .unwrap_or(PROTOCOL_VERSION);
            Reply {
                status: 200,
                body: Some(json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "protocolVersion": requested,
                        "capabilities": {"tools": {"listChanged": false}},
                        "serverInfo": {"name": info.name, "version": info.version},
                    },
                })),
                is_initialize: true,
            }
        }
        Some("ping") => reply(id, json!({})),
        Some("tools/list") => {
            let listed: Vec<Value> = tools
                .iter()
                .map(|tool| {
                    json!({
                        "name": tool.name,
                        "description": tool.description,
                        "inputSchema": tool.input_schema,
                    })
                })
                .collect();
            reply(id, json!({"tools": listed}))
        }
        Some("tools/call") => call_tool(id, tools, &params),
        other => {
            // A notification carries no id and needs no response — just acknowledge it.
            if id.is_null() {
                return Reply::new(202, None);
            }
            fail(
                id,
                -32601,
                &format!("method not found: {}", other.unwrap_or("(none)")),
            )
        }
    }
}

fn call_tool(id: Value, tools: &[Tool], params: &Value) -> Reply {
    let name = params.get("name").and_then(Value::as_str).unwrap_or("");
    let Some(tool) = tools.iter().find(|tool| tool.name == name) else {
        let shown = if name.is_empty() { "(none)" } else { name };
        return fail(id, -32602, &format!("unknown tool: {shown}"));
    };

    let args = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
    // A failing tool is a tool-level error, not a protocol one: the client still gets a
    // valid result envelope, flagged with isError.
    match (tool.handler)(&args) {
        Ok(result) => reply(id, json!({"content": [text(&result)]})),
        Err(detail) => reply(
            id,
            json!({
                "content": [text(&Value::String(format!("error: {detail}")))],
                "isError": true,
            }),
        ),
    }
}

fn text(result: &Value) -> Value {
    let body = match result {
        Value::String(s) => s.clone(),
        other => serde_json::to_string_pretty(other).unwrap_or_else(|e| e.to_string()),
    };
    json!({"type": "text", "text": body})
}

fn reply(id: Value, result: Value) -> Reply {
    Reply::new(200, Some(json!({"jsonrpc": "2.0", "id": id, "result": result})))
}

/// JSON-RPC errors ride a 200 with an `error` member; transport failures use non-200.
fn fail(id: Value, code: i32, message: &str) -> Reply {
    Reply::new(
        200,
        Some(json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": {"code": code, "message": message},
        })),
    )
}

/// Compact one-line label for the debug log ("tools/call get_logs", "initialize", …).
pub fn label(message: &Value) -> String {
    let method = message
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    match message
        .get("params")
        .and_then(|p| p.get("name"))
        .and_then(Value::as_str)
    {
        Some(name) if method == "tools/call" => format!("tools/call {name}"),
        _ => method.to_string(),
    }
}

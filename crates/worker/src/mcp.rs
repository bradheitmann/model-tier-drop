//! `POST /mcp`: header checks, body parsing and the JSON-RPC method switch.
//!
//! MCP over Streamable HTTP with JSON responses, stateless. Accepts the `initialize` handshake
//! (2025-03-26, 2025-06-18, 2025-11-25) and the per-request `_meta` form of 2026-07-28.
//! Nothing here depends on `worker` types, so it is unit-tested on the host target.

use serde_json::{json, Map, Value};

use crate::assets::{SERVER_NAME, VERSION};
use crate::reply::Reply;
use crate::tools;

/// The stateless protocol version, sent per request in `params._meta`.
pub const STATELESS_VERSION: &str = "2026-07-28";
/// Versions accepted by the `initialize` handshake, newest first.
pub const HANDSHAKE_VERSIONS: [&str; 3] = ["2025-11-25", "2025-06-18", "2025-03-26"];
/// The `_meta` key that carries the stateless version.
pub const META_VERSION: &str = "io.modelcontextprotocol/protocolVersion";
/// Largest accepted request body, in bytes (256 KiB).
pub const MAX_BODY: usize = 262_144;
/// How long a client may cache `tools/list`: one hour. The list never depends on the caller.
pub const TOOLS_TTL_MS: u64 = 3_600_000;

/// Text returned in `initialize` and `server/discover`.
pub const INSTRUCTIONS: &str = "model-tier-drop routes each model-using task to the lowest tier that meets its quality floors and moves up only on recorded evidence. Read the skill first with skill_read (SKILL.md, then the references it names) before calling plan_next, check_floors, assess_disqualification or decide. The server is stateless and stores nothing: pass in the records you hold and keep the returned records in your own ledger.";

/// The request facts the header checks read; `lib.rs` fills it from the `worker::Request`.
#[derive(Debug, Default, Clone)]
pub struct PostMeta<'a> {
    pub method: &'a str,
    /// Origin of the request URL (scheme, host and port).
    pub self_origin: &'a str,
    pub origin: Option<&'a str>,
    pub accept: Option<&'a str>,
    pub content_type: Option<&'a str>,
    pub content_length: Option<&'a str>,
}

fn all_versions() -> Vec<&'static str> {
    let mut v = vec![STATELESS_VERSION];
    v.extend(HANDSHAKE_VERSIONS);
    v
}

fn rpc_error(id: Value, code: i64, message: &str, data: Option<Value>) -> Value {
    let mut error = json!({ "code": code, "message": message });
    if let Some(d) = data {
        error["data"] = d;
    }
    json!({ "jsonrpc": "2.0", "id": id, "error": error })
}

/// Checks before the body is read. `Some` is the refusal; `None` means read the body.
pub fn preflight(m: &PostMeta) -> Option<Reply> {
    if m.method != "POST" {
        return Some(Reply::method_not_allowed("POST"));
    }
    // Browsers must not drive this server from other sites (DNS rebinding, CSRF); CLI and agent
    // clients send no Origin.
    if let Some(o) = m.origin {
        if o != m.self_origin {
            return Some(Reply::json(403, &json!({ "error": "Origin denied" })));
        }
    }
    if let Some(a) = m.accept {
        let a = a.to_ascii_lowercase();
        if !a.contains("application/json") && !a.contains("*/*") {
            return Some(Reply::json(
                406,
                &rpc_error(
                    Value::Null,
                    -32600,
                    "Accept must include application/json",
                    None,
                ),
            ));
        }
    }
    // decision: the media type is compared case-insensitively (RFC 9110 media types are
    // case-insensitive); the reference compares exactly.
    let media = m
        .content_type
        .unwrap_or("")
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    if media != "application/json" {
        return Some(Reply::json(
            415,
            &json!({ "error": "Content-Type must be application/json" }),
        ));
    }
    let declared = m
        .content_length
        .and_then(|l| l.trim().parse::<u64>().ok())
        .unwrap_or(0);
    if declared > MAX_BODY as u64 {
        return Some(too_large());
    }
    None
}

fn too_large() -> Reply {
    Reply::json(413, &json!({ "error": "Request too large" }))
}

/// Parse error reply (also used by `lib.rs` when the body cannot be read).
pub fn parse_error() -> Reply {
    Reply::json(400, &rpc_error(Value::Null, -32700, "Parse error", None))
}

/// The body after `preflight`: size cap, UTF-8, JSON, then the JSON-RPC switch.
pub fn handle_body(bytes: &[u8]) -> Reply {
    if bytes.len() > MAX_BODY {
        return too_large();
    }
    let Ok(text) = std::str::from_utf8(bytes) else {
        return parse_error();
    };
    match serde_json::from_str::<Value>(text) {
        Ok(v) => rpc(v),
        Err(_) => parse_error(),
    }
}

fn is_valid_id(id: &Value) -> bool {
    const MAX_SAFE: i64 = 9_007_199_254_740_991;
    match id {
        Value::String(_) => true,
        Value::Number(n) => n
            .as_i64()
            .map(|i| (-MAX_SAFE..=MAX_SAFE).contains(&i))
            .unwrap_or(false),
        _ => false,
    }
}

/// The JSON-RPC method switch over one parsed message.
pub fn rpc(body: Value) -> Reply {
    if body.is_array() {
        return Reply::json(
            400,
            &rpc_error(
                Value::Null,
                -32600,
                "Batch requests are not supported",
                None,
            ),
        );
    }
    let Some(obj) = body.as_object() else {
        return invalid_request();
    };
    let Some(method) = obj.get("method").and_then(Value::as_str) else {
        return invalid_request();
    };
    if obj.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        return invalid_request();
    }
    // Notifications (no id), including notifications/initialized, are accepted without a body.
    let Some(id) = obj.get("id").cloned() else {
        return Reply::empty(202);
    };
    if !is_valid_id(&id) {
        return Reply::json(
            400,
            &rpc_error(
                Value::Null,
                -32600,
                "Request id must be a string or integer",
                None,
            ),
        );
    }
    let empty = Map::new();
    let params = obj
        .get("params")
        .and_then(Value::as_object)
        .unwrap_or(&empty);
    let meta = params
        .get("_meta")
        .and_then(Value::as_object)
        .unwrap_or(&empty);
    let stateless = meta.get(META_VERSION);
    if let Some(v) = stateless {
        if v.as_str() != Some(STATELESS_VERSION) {
            return Reply::json(
                400,
                &rpc_error(
                    id,
                    -32022,
                    "Unsupported protocol version",
                    Some(json!({ "supportedVersions": all_versions() })),
                ),
            );
        }
    }
    let done = |result: Value| {
        let result = if stateless.is_some() {
            let mut m = Map::new();
            m.insert("resultType".into(), json!("complete"));
            if let Value::Object(r) = result {
                m.extend(r);
            }
            Value::Object(m)
        } else {
            result
        };
        Reply::json(
            200,
            &json!({ "jsonrpc": "2.0", "id": id.clone(), "result": result }),
        )
    };
    match method {
        "initialize" => {
            let requested = params
                .get("protocolVersion")
                .and_then(Value::as_str)
                .unwrap_or("");
            let protocol_version = if all_versions().contains(&requested) {
                requested
            } else {
                HANDSHAKE_VERSIONS[0]
            };
            done(json!({
                "protocolVersion": protocol_version,
                "capabilities": { "tools": { "listChanged": false } },
                "serverInfo": { "name": SERVER_NAME, "version": VERSION },
                "instructions": INSTRUCTIONS,
            }))
        }
        "server/discover" => done(json!({
            "supportedVersions": all_versions(),
            "capabilities": { "tools": {} },
            "instructions": INSTRUCTIONS,
            "_meta": { "io.modelcontextprotocol/serverInfo": { "name": SERVER_NAME, "version": VERSION } },
        })),
        "ping" => done(json!({})),
        "tools/list" => done(json!({
            "tools": tools::list(),
            "ttlMs": TOOLS_TTL_MS,
            "cacheScope": "public",
        })),
        "tools/call" => {
            let Some(name) = params.get("name").and_then(Value::as_str) else {
                return Reply::json(
                    400,
                    &rpc_error(id, -32602, "tools/call needs a tool name", None),
                );
            };
            let args = match params.get("arguments") {
                None | Some(Value::Null) => json!({}),
                Some(a) => a.clone(),
            };
            // decision: a successful structuredContent is the tool's output itself, without the
            // reference's merged `ok: true`, so records (the example registry, plans, decisions)
            // can be passed back as inputs that deny unknown fields. Failures are
            // `{ ok: false, error: { code, message } }`.
            let (structured, is_error) = match tools::call(name, args) {
                Ok(v) => (v, false),
                Err(e) => (
                    json!({ "ok": false, "error": { "code": e.code, "message": e.message } }),
                    true,
                ),
            };
            let text = serde_json::to_string(&structured).unwrap_or_default();
            done(json!({
                "content": [{ "type": "text", "text": text }],
                "structuredContent": structured,
                "isError": is_error,
            }))
        }
        other => Reply::json(
            404,
            &rpc_error(id, -32601, &format!("Method not found: {other}"), None),
        ),
    }
}

fn invalid_request() -> Reply {
    Reply::json(
        400,
        &rpc_error(Value::Null, -32600, "Invalid JSON-RPC request", None),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::tests::{classify_task, example};

    fn post<'a>() -> PostMeta<'a> {
        PostMeta {
            method: "POST",
            self_origin: "http://localhost:8787",
            content_type: Some("application/json"),
            ..PostMeta::default()
        }
    }

    fn call(body: Value) -> Reply {
        rpc(body)
    }

    #[test]
    fn preflight_codes() {
        assert!(preflight(&post()).is_none());
        let mut m = post();
        m.method = "GET";
        assert_eq!(preflight(&m).unwrap().status, 405);
        let mut m = post();
        m.origin = Some("https://evil.example");
        assert_eq!(preflight(&m).unwrap().status, 403);
        let mut m = post();
        m.origin = Some("http://localhost:8787");
        assert!(preflight(&m).is_none());
        let mut m = post();
        m.accept = Some("text/html");
        assert_eq!(preflight(&m).unwrap().status, 406);
        let mut m = post();
        m.accept = Some("application/json, text/event-stream");
        assert!(preflight(&m).is_none());
        let mut m = post();
        m.content_type = Some("text/plain");
        assert_eq!(preflight(&m).unwrap().status, 415);
        let mut m = post();
        m.content_type = Some("Application/JSON; charset=utf-8");
        assert!(preflight(&m).is_none());
        let mut m = post();
        m.content_length = Some("300000");
        assert_eq!(preflight(&m).unwrap().status, 413);
    }

    #[test]
    fn body_codes() {
        assert_eq!(handle_body(&vec![b' '; MAX_BODY + 1]).status, 413);
        assert_eq!(handle_body(b"{nope").status, 400);
        assert_eq!(
            handle_body(&[0xff, 0xfe]).json_body()["error"]["code"],
            -32700
        );
        let r = handle_body(b"[]");
        assert_eq!(r.status, 400);
        assert_eq!(r.json_body()["error"]["code"], -32600);
        assert_eq!(r.header("Cache-Control"), Some("no-store"));
    }

    #[test]
    fn initialize_echoes_known_versions() {
        for v in ["2025-11-25", "2025-06-18", "2025-03-26", "2026-07-28"] {
            let r = call(
                json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "protocolVersion": v } }),
            );
            let b = r.json_body();
            assert_eq!(r.status, 200);
            assert_eq!(b["result"]["protocolVersion"], v);
            assert_eq!(b["result"]["serverInfo"]["name"], "model-tier-drop");
            assert_eq!(b["result"]["capabilities"]["tools"]["listChanged"], false);
        }
        let r = call(
            json!({ "jsonrpc": "2.0", "id": "x", "method": "initialize", "params": { "protocolVersion": "1999-01-01" } }),
        );
        assert_eq!(r.json_body()["result"]["protocolVersion"], "2025-11-25");
    }

    #[test]
    fn stateless_meta() {
        let r = call(
            json!({ "jsonrpc": "2.0", "id": 1, "method": "ping", "params": { "_meta": { META_VERSION: STATELESS_VERSION } } }),
        );
        assert_eq!(r.json_body()["result"]["resultType"], "complete");
        let r = call(
            json!({ "jsonrpc": "2.0", "id": 1, "method": "ping", "params": { "_meta": { META_VERSION: "2020-01-01" } } }),
        );
        assert_eq!(r.status, 400);
        assert_eq!(r.json_body()["error"]["code"], -32022);
    }

    #[test]
    fn notifications_unknown_and_invalid() {
        let r = call(json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }));
        assert_eq!(r.status, 202);
        assert!(r.body.is_none());
        let r = call(json!({ "jsonrpc": "2.0", "id": 2, "method": "nope" }));
        assert_eq!(r.status, 404);
        assert_eq!(r.json_body()["error"]["code"], -32601);
        assert_eq!(
            call(json!({ "jsonrpc": "1.0", "id": 2, "method": "ping" })).status,
            400
        );
        assert_eq!(
            call(json!({ "jsonrpc": "2.0", "id": null, "method": "ping" })).status,
            400
        );
        assert_eq!(
            call(json!({ "jsonrpc": "2.0", "id": 1.5, "method": "ping" })).status,
            400
        );
        assert_eq!(
            call(json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {} })).status,
            400
        );
    }

    #[test]
    fn tools_list_fields() {
        let b = call(json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" })).json_body();
        assert_eq!(b["result"]["ttlMs"], 3_600_000);
        assert_eq!(b["result"]["cacheScope"], "public");
        assert_eq!(b["result"]["tools"].as_array().unwrap().len(), 13);
    }

    #[test]
    fn tools_call_success_and_error() {
        let r = call(json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call",
            "params": { "name": "plan_next", "arguments": { "task": classify_task(), "registry": example() } } }));
        let b = r.json_body();
        assert_eq!(r.status, 200);
        assert_eq!(b["result"]["isError"], false);
        assert_eq!(b["result"]["structuredContent"]["tier"], "system_one");
        let text: Value =
            serde_json::from_str(b["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(text, b["result"]["structuredContent"]);

        let r = call(json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/call",
            "params": { "name": "decide", "arguments": { "garbage": true } } }));
        let b = r.json_body();
        assert_eq!(r.status, 200);
        assert_eq!(b["result"]["isError"], true);
        assert_eq!(b["result"]["structuredContent"]["ok"], false);
        assert_eq!(
            b["result"]["structuredContent"]["error"]["code"],
            "invalid_arguments"
        );
    }
}

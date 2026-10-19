//! model-tier-drop MCP Worker (workers-rs).
//!
//! `POST /mcp` answers MCP over Streamable HTTP with JSON responses and exposes the engine's pure
//! operations as twelve tools. GET routes serve the skill, the example registry, the default policy
//! and the record schemas from bytes embedded at compile time. The server is stateless: no
//! bindings, no secrets, no storage, no auth.
//!
//! Layout: `reply` (a plain response value), `mcp` (header checks and the JSON-RPC switch),
//! `tools` (the twelve tools), `assets` (embedded files and GET routes). Only this file touches
//! `worker` types, so the rest is unit-tested on the host target with `cargo test`.

mod assets;
mod mcp;
mod reply;
mod tools;

use worker::{event, Context, Env, Headers, Request, Response, Result};

use crate::reply::Reply;

#[event(fetch)]
async fn fetch(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    console_error_panic_hook::set_once();
    handle(req, env).await
}

/// Route one request.
pub async fn handle(mut req: Request, _env: Env) -> Result<Response> {
    let url = req.url()?;
    let self_origin = url.origin().ascii_serialization();
    let path = url.path().to_string();
    let method = req.method().to_string();
    let reply = if path == "/mcp" {
        let h = req.headers();
        let get = |name: &str| h.get(name).ok().flatten();
        let (origin, accept, content_type, content_length) = (
            get("origin"),
            get("accept"),
            get("content-type"),
            get("content-length"),
        );
        let meta = mcp::PostMeta {
            method: &method,
            self_origin: &self_origin,
            origin: origin.as_deref(),
            accept: accept.as_deref(),
            content_type: content_type.as_deref(),
            content_length: content_length.as_deref(),
        };
        match mcp::preflight(&meta) {
            Some(refusal) => refusal,
            None => match req.bytes().await {
                Ok(bytes) => mcp::handle_body(&bytes),
                Err(_) => mcp::parse_error(),
            },
        }
    } else {
        assets::get_route(&method, &path, &self_origin)
    };
    to_response(reply)
}

fn to_response(reply: Reply) -> Result<Response> {
    let headers = Headers::new();
    for (name, value) in &reply.headers {
        headers.set(name, value)?;
    }
    let builder = Response::builder()
        .with_status(reply.status)
        .with_headers(headers);
    Ok(match reply.body {
        Some(body) => builder.fixed(body),
        None => builder.empty(),
    })
}

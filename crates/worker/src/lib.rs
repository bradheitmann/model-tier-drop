//! model-tier-drop MCP Worker. Filled in by slice S-05.
use worker::{event, Context, Env, Request, Response, Result};

#[event(fetch)]
async fn fetch(_req: Request, _env: Env, _ctx: Context) -> Result<Response> {
    Response::ok("model-tier-drop: not implemented yet (slice S-05)")
}

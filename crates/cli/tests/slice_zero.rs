//! Slice Zero: the tiniest end-to-end live run (charter/charter.toml `[slice_zero]`).
//!
//! The README walkthrough task is planned twice: by the local `tier-drop plan` over the example
//! registry, and by the live hosted MCP's `plan_next` tool over the same task and registry. The two
//! plans must be byte-identical after key sorting. The test needs the network and `curl`, so it is
//! ignored by default; run it with
//! `cargo test -p tier-drop-cli --test slice_zero -- --ignored`.
//! `TIER_DROP_MCP_URL` overrides the endpoint (default: the hosted MCP named in the README).

use std::io::Write;
use std::process::{Command, Stdio};

use serde_json::{json, Value};

const README: &str = include_str!("../../../README.md");
const BIN: &str = env!("CARGO_BIN_EXE_tier-drop");
const DEFAULT_MCP: &str = "https://model-tier-drop-mcp.bradheitmann.ai/mcp";

/// The JSON body of the README's `cat > task.json <<'EOF'` heredoc.
fn readme_task() -> Value {
    let start = README
        .find("cat > task.json <<'EOF'\n")
        .expect("README walkthrough writes task.json");
    let body = &README[start..];
    let body = &body[body.find('\n').unwrap() + 1..];
    let end = body.find("\nEOF").expect("task.json heredoc ends with EOF");
    serde_json::from_str(&body[..end]).expect("README task.json is JSON")
}

fn local_plan(task: &Value) -> (Value, Value) {
    let dir = std::env::temp_dir().join(format!("tier-drop-slice-zero-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let registry = Command::new(BIN)
        .args(["registry", "example"])
        .output()
        .unwrap();
    assert!(
        registry.status.success(),
        "tier-drop registry example failed"
    );
    std::fs::write(dir.join("registry.json"), &registry.stdout).unwrap();
    std::fs::write(dir.join("task.json"), task.to_string()).unwrap();
    let plan = Command::new(BIN)
        .args([
            "plan",
            "--task",
            "task.json",
            "--registry",
            "registry.json",
            "--ledger",
            "ledger",
        ])
        .current_dir(&dir)
        .output()
        .unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        plan.status.success(),
        "tier-drop plan failed: {}",
        String::from_utf8_lossy(&plan.stderr)
    );
    (
        serde_json::from_slice(&registry.stdout).unwrap(),
        serde_json::from_slice(&plan.stdout).unwrap(),
    )
}

fn remote_plan(url: &str, task: &Value, registry: &Value) -> Value {
    let request = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": "plan_next", "arguments": { "task": task, "registry": registry } }
    });
    let mut curl = Command::new("curl")
        .args([
            "-sS",
            "-m",
            "30",
            url,
            "-H",
            "content-type: application/json",
            "-H",
            "accept: application/json, text/event-stream",
            "--data-binary",
            "@-",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("curl is installed");
    curl.stdin
        .take()
        .unwrap()
        .write_all(request.to_string().as_bytes())
        .unwrap();
    let out = curl.wait_with_output().unwrap();
    assert!(out.status.success(), "curl to {url} failed");
    let response: Value = serde_json::from_slice(&out.stdout).expect("MCP answered JSON");
    assert_eq!(
        response["result"]["isError"],
        Value::Bool(false),
        "plan_next returned an error: {response}"
    );
    response["result"]["structuredContent"].clone()
}

#[test]
#[ignore = "live network run against the hosted MCP"]
fn local_cli_and_live_mcp_plan_the_same() {
    let url = std::env::var("TIER_DROP_MCP_URL").unwrap_or_else(|_| DEFAULT_MCP.to_string());
    let task = readme_task();
    let (registry, local) = local_plan(&task);
    let remote = remote_plan(&url, &task, &registry);
    assert_eq!(local["kind"], "try", "the README task must plan a try");
    // serde_json's default map is sorted, so Value equality is key-order independent.
    assert_eq!(local, remote, "local CLI and live MCP disagree");
    println!(
        "slice-zero ok: {url} tier={} candidates={}",
        local["tier"],
        local["candidates"].as_array().map_or(0, Vec::len)
    );
}

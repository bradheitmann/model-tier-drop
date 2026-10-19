//! Bytes embedded at compile time and the GET routes that serve them.
//!
//! The skill, the example registry and the default policy are `include_str!`ed from the
//! repository, so the served bytes equal the repository's files at the commit that was built.

use serde_json::json;

use crate::reply::{Reply, PUBLIC_5_MIN};

/// `skill/SKILL.md`.
pub const SKILL_MD: &str = include_str!("../../../skill/SKILL.md");

/// Every file under `skill/references/`, by file name without `.md`.
// decision: the list is explicit (no build script), so adding a reference file means adding a
// line here; the test `every_reference_file_is_embedded` fails when the directory and the list differ.
pub const REFERENCES: &[(&str, &str)] = &[
    (
        "composition",
        include_str!("../../../skill/references/composition.md"),
    ),
    (
        "dataset",
        include_str!("../../../skill/references/dataset.md"),
    ),
    (
        "eval-harness",
        include_str!("../../../skill/references/eval-harness.md"),
    ),
    (
        "registry",
        include_str!("../../../skill/references/registry.md"),
    ),
    (
        "routing-record",
        include_str!("../../../skill/references/routing-record.md"),
    ),
    ("tiers", include_str!("../../../skill/references/tiers.md")),
];

/// `registry/registry.example.json`.
pub const REGISTRY_EXAMPLE_JSON: &str = include_str!("../../../registry/registry.example.json");

/// `registry/policy.default.json`.
pub const POLICY_DEFAULT_JSON: &str = include_str!("../../../registry/policy.default.json");

/// The server name used in `serverInfo` and the info document.
pub const SERVER_NAME: &str = "model-tier-drop";

/// The crate version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

const MARKDOWN: &str = "text/markdown; charset=utf-8";
const JSON: &str = "application/json";

/// The paths `skill_read` accepts: `SKILL.md` and `references/<name>.md`.
pub fn skill_paths() -> Vec<String> {
    let mut out = vec!["SKILL.md".to_string()];
    out.extend(REFERENCES.iter().map(|(n, _)| format!("references/{n}.md")));
    out
}

/// The embedded text at a skill path, if there is one.
pub fn skill_text(path: &str) -> Option<&'static str> {
    if path == "SKILL.md" {
        return Some(SKILL_MD);
    }
    let name = path.strip_prefix("references/")?.strip_suffix(".md")?;
    REFERENCES.iter().find(|(n, _)| *n == name).map(|(_, t)| *t)
}

/// One record schema as pretty JSON, by the names the `schemas` tool accepts.
pub fn schema_json(name: &str) -> Option<String> {
    let all = tier_drop_core::schemas();
    let schema = all.get(name)?;
    serde_json::to_string_pretty(schema.as_value()).ok()
}

/// Every route except `/mcp`. `self_origin` is the origin of the request URL.
pub fn get_route(method: &str, path: &str, self_origin: &str) -> Reply {
    let found = match path {
        "" | "/" => Some(Reply::json(
            200,
            &json!({
                "name": SERVER_NAME,
                "version": VERSION,
                "mcp": format!("{self_origin}/mcp"),
                "skill": format!("{self_origin}/skill/SKILL.md"),
            }),
        )),
        "/robots.txt" => Some(text(
            "text/plain; charset=utf-8",
            "User-agent: *\nDisallow: /\n",
        )),
        "/skill/SKILL.md" => Some(text(MARKDOWN, SKILL_MD)),
        "/registry/example.json" => Some(text(JSON, REGISTRY_EXAMPLE_JSON)),
        "/policy/default.json" => Some(text(JSON, POLICY_DEFAULT_JSON)),
        _ => {
            if let Some(rest) = path.strip_prefix("/skill/") {
                // Only `references/<embedded name>.md`; `SKILL.md` was matched above.
                rest.strip_prefix("references/")
                    .and_then(|_| skill_text(rest))
                    .map(|t| text(MARKDOWN, t))
            } else if let Some(name) = path
                .strip_prefix("/schemas/")
                .and_then(|r| r.strip_suffix(".json"))
            {
                schema_json(name).map(|s| Reply::bytes(200, JSON, PUBLIC_5_MIN, s.into_bytes()))
            } else {
                None
            }
        }
    };
    match found {
        // decision: unknown paths answer 404 whatever the method; known paths accept GET and HEAD only.
        None => Reply::not_found(),
        Some(_) if method != "GET" && method != "HEAD" => Reply::method_not_allowed("GET, HEAD"),
        Some(r) => r,
    }
}

fn text(content_type: &str, body: &str) -> Reply {
    Reply::bytes(200, content_type, PUBLIC_5_MIN, body.as_bytes().to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reply::NO_STORE;

    #[test]
    fn every_reference_file_is_embedded() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../skill/references");
        let mut on_disk: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".md"))
            .map(|n| n.trim_end_matches(".md").to_string())
            .collect();
        on_disk.sort();
        let embedded: Vec<String> = REFERENCES.iter().map(|(n, _)| n.to_string()).collect();
        assert_eq!(on_disk, embedded);
    }

    #[test]
    fn skill_is_served_byte_for_byte() {
        let r = get_route("GET", "/skill/SKILL.md", "http://x");
        let disk = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../skill/SKILL.md"));
        assert_eq!(r.status, 200);
        assert_eq!(r.body.as_ref().unwrap(), &disk.unwrap());
        assert_eq!(r.header("Cache-Control"), Some(PUBLIC_5_MIN));
        assert_eq!(r.header("X-Robots-Tag"), Some("noindex, nofollow"));
    }

    #[test]
    fn references_only_embedded_names() {
        assert_eq!(
            get_route("GET", "/skill/references/tiers.md", "http://x").status,
            200
        );
        for p in [
            "/skill/references/nope.md",
            "/skill/references/../SKILL.md",
            "/skill/references/tiers",
            "/skill/tiers.md",
            "/skill/",
            "/schemas/nope.json",
            "/schemas/task",
            "/elsewhere",
        ] {
            let r = get_route("GET", p, "http://x");
            assert_eq!(r.status, 404, "{p}");
            assert_eq!(r.json_body()["error"], "Not found");
        }
    }

    #[test]
    fn schema_route_has_title() {
        let r = get_route("GET", "/schemas/routing_decision.json", "http://x");
        assert_eq!(r.status, 200);
        assert_eq!(r.json_body()["title"], "RoutingDecision");
        for name in tier_drop_core::schemas().keys() {
            let p = format!("/schemas/{name}.json");
            assert_eq!(get_route("GET", &p, "http://x").status, 200, "{p}");
        }
    }

    #[test]
    fn info_and_methods() {
        let r = get_route("GET", "/", "https://h.example");
        assert_eq!(r.json_body()["mcp"], "https://h.example/mcp");
        assert_eq!(r.json_body()["name"], SERVER_NAME);
        assert_eq!(r.header("Cache-Control"), Some(NO_STORE));
        assert_eq!(get_route("POST", "/robots.txt", "http://x").status, 405);
        assert_eq!(get_route("HEAD", "/robots.txt", "http://x").status, 200);
    }

    #[test]
    fn skill_text_lookup() {
        assert_eq!(skill_paths().len(), 1 + REFERENCES.len());
        for p in skill_paths() {
            assert!(skill_text(&p).is_some(), "{p}");
        }
        assert!(skill_text("references/nope.md").is_none());
        assert!(skill_text("../SKILL.md").is_none());
    }
}

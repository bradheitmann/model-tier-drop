//! A plain response value. The routing code builds `Reply`s without touching `worker` types, so
//! it runs (and is unit-tested) on the host target; `lib.rs` turns a `Reply` into a `Response`.

use serde_json::Value;

/// `Cache-Control` for the MCP endpoint, errors and the info document.
pub const NO_STORE: &str = "no-store";
/// `Cache-Control` for the embedded text routes (skill, registry, policy, schemas, robots).
pub const PUBLIC_5_MIN: &str = "public, max-age=300";

/// Status, headers and an optional body.
#[derive(Debug, Clone, PartialEq)]
pub struct Reply {
    pub status: u16,
    pub headers: Vec<(&'static str, String)>,
    pub body: Option<Vec<u8>>,
}

/// Headers carried by every response.
fn security_headers() -> Vec<(&'static str, String)> {
    vec![
        ("X-Robots-Tag", "noindex, nofollow".to_string()),
        ("X-Content-Type-Options", "nosniff".to_string()),
        ("Referrer-Policy", "no-referrer".to_string()),
    ]
}

impl Reply {
    /// A JSON body with `Cache-Control: no-store`.
    pub fn json(status: u16, value: &Value) -> Reply {
        // decision: serialising a `serde_json::Value` cannot fail, so an empty body is unreachable.
        let body = serde_json::to_vec(value).unwrap_or_default();
        Reply::bytes(status, "application/json", NO_STORE, body)
    }

    /// A body with the given content type and cache policy.
    pub fn bytes(status: u16, content_type: &str, cache: &str, body: Vec<u8>) -> Reply {
        let mut headers = security_headers();
        headers.push(("Content-Type", content_type.to_string()));
        headers.push(("Cache-Control", cache.to_string()));
        Reply {
            status,
            headers,
            body: Some(body),
        }
    }

    /// No body; security headers only (notifications answer 202 this way).
    pub fn empty(status: u16) -> Reply {
        let mut headers = security_headers();
        headers.push(("Cache-Control", NO_STORE.to_string()));
        Reply {
            status,
            headers,
            body: None,
        }
    }

    /// The JSON 404 used for every unknown path.
    pub fn not_found() -> Reply {
        Reply::json(404, &serde_json::json!({ "error": "Not found" }))
    }

    /// 405 with an `Allow` header.
    pub fn method_not_allowed(allow: &str) -> Reply {
        Reply::empty(405).with_header("Allow", allow)
    }

    /// Add or replace a header.
    pub fn with_header(mut self, name: &'static str, value: &str) -> Reply {
        self.headers.retain(|(n, _)| !n.eq_ignore_ascii_case(name));
        self.headers.push((name, value.to_string()));
        self
    }

    /// A header value, case-insensitive.
    #[cfg(test)]
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    /// The body parsed as JSON.
    #[cfg(test)]
    pub fn json_body(&self) -> Value {
        serde_json::from_slice(self.body.as_deref().unwrap_or_default()).unwrap()
    }
}

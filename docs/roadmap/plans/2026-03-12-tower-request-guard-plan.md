# tower-request-guard Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a Tower middleware that validates requests (body size, timeout, content-type, required headers, JSON depth) before they reach the handler.

**Architecture:** Monolithic service — one Layer, one Service, all validations executed in sequence (cheapest first). Per-route overrides via request extensions. Feature-gated JSON depth validation with buffered body variant.

**Tech Stack:** Rust, Tower 0.5, http 1.x, tokio 1.x, tracing 0.1, serde_json 1.x (optional via `json` feature)

**Spec:** `docs/roadmap/design/2026-03-12-tower-request-guard-design.md`

**Reference crate:** `tower-rate-tier` — same author, same patterns. Mirror its Layer/Service/Builder structure.

---

## Chunk 1: Foundation

### Task 1: Project Scaffold

**Files:**
- Create: `Cargo.toml`
- Create: `src/lib.rs`
- Create: `LICENSE`

- [ ] **Step 1: Create Cargo.toml**

```toml
[package]
name = "tower-request-guard"
version = "0.1.0"
edition = "2021"
license = "MIT OR Apache-2.0"
description = "Request validation middleware for Tower"
repository = "https://github.com/SoftDryzz/tower-request-guard"
keywords = ["tower", "middleware", "validation", "request-guard", "security"]
categories = ["web-programming", "network-programming"]

[dependencies]
tower = { version = "0.5", features = ["util"] }
tower-layer = "0.3"
tower-service = "0.3"
http = "1"
http-body = "1"
bytes = "1"
tokio = { version = "1", features = ["time"] }
pin-project-lite = "0.2"
tracing = "0.1"

[dependencies.http-body-util]
version = "0.1"
optional = true

[dependencies.serde_json]
version = "1"
optional = true

[dev-dependencies]
axum = "0.8"
tokio = { version = "1", features = ["full", "test-util"] }
hyper = "1"
tower = { version = "0.5", features = ["util"] }
serde_json = "1"

[features]
default = []
json = ["dep:serde_json", "dep:http-body-util"]
```

- [ ] **Step 2: Create LICENSE**

```
MIT OR Apache-2.0
```

Use the standard dual-license text. Copy format from tower-rate-tier's LICENSE file.

- [ ] **Step 3: Create src/lib.rs with module declarations**

```rust
//! Request validation middleware for [`tower`] services.
//!
//! `tower-request-guard` validates incoming requests before they reach
//! the handler: body size limits, timeouts, content-type enforcement,
//! required headers, and JSON depth protection.

pub mod body;
pub mod content_type;
pub mod guard;
pub mod headers;
pub mod layer;
pub mod response;
pub mod route;
pub mod service;
pub mod violation;

#[cfg(feature = "json")]
pub mod json;

#[cfg(feature = "json")]
pub mod buffered;

// Re-exports
pub use guard::RequestGuard;
pub use layer::RequestGuardLayer;
pub use route::route_guard;
pub use service::RequestGuardService;
pub use violation::{OnViolation, Violation, ViolationAction};

#[cfg(feature = "json")]
pub use buffered::{BufferedRequestGuardLayer, BufferedRequestGuardService};
```

- [ ] **Step 4: Create stub modules so crate compiles**

Create empty files for every module declared in lib.rs so `cargo check` passes:

`src/body.rs`, `src/content_type.rs`, `src/guard.rs`, `src/headers.rs`, `src/layer.rs`, `src/response.rs`, `src/route.rs`, `src/service.rs`, `src/violation.rs`, `src/json.rs`, `src/buffered.rs`

**Note:** `src/json.rs` and `src/buffered.rs` are feature-gated but must exist as empty files so `cargo check --features json` passes.

Each file starts empty (will be filled in subsequent tasks).

- [ ] **Step 5: Run cargo check**

Run: `cargo check`
Expected: Compiles with warnings about empty modules

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml LICENSE src/
git commit -m "feat: scaffold project with module structure and dependencies"
```

---

### Task 2: Violation Types

**Files:**
- Create: `src/violation.rs`
- Test: inline `#[cfg(test)]` module

- [ ] **Step 1: Write tests for Violation enum and OnViolation**

```rust
// src/violation.rs

#[cfg(test)]
mod tests {
    use super::*;
    use http::StatusCode;

    #[test]
    fn violation_status_codes() {
        assert_eq!(Violation::BodyTooLarge { max: 100, received: 200 }.status_code(), StatusCode::PAYLOAD_TOO_LARGE);
        assert_eq!(Violation::RequestTimeout { timeout_ms: 5000 }.status_code(), StatusCode::GATEWAY_TIMEOUT);
        assert_eq!(
            Violation::InvalidContentType {
                received: "text/xml".into(),
                allowed: vec!["application/json".into()],
            }.status_code(),
            StatusCode::UNSUPPORTED_MEDIA_TYPE
        );
        assert_eq!(
            Violation::MissingHeader { header: "Authorization".into() }.status_code(),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            Violation::JsonTooDeep { max_depth: 32, found_depth: 128 }.status_code(),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            Violation::InvalidJson { detail: "unexpected EOF".into() }.status_code(),
            StatusCode::BAD_REQUEST
        );
    }

    #[test]
    fn on_violation_default_is_reject() {
        assert!(matches!(OnViolation::default(), OnViolation::Reject));
    }

    #[test]
    fn violation_action_default_is_reject() {
        assert!(matches!(ViolationAction::default(), ViolationAction::Reject));
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib violation::tests`
Expected: FAIL — types not defined

- [ ] **Step 3: Implement Violation, OnViolation, ViolationAction**

```rust
// src/violation.rs

use http::{Response, StatusCode};
use std::sync::Arc;

/// A request validation violation detected by the guard.
#[derive(Debug, Clone)]
pub enum Violation {
    BodyTooLarge {
        max: u64,
        received: u64,
    },
    RequestTimeout {
        timeout_ms: u64,
    },
    InvalidContentType {
        received: String,
        allowed: Vec<String>,
    },
    MissingHeader {
        header: String,
    },
    JsonTooDeep {
        max_depth: u32,
        found_depth: u32,
    },
    InvalidJson {
        detail: String,
    },
}

impl Violation {
    /// Returns the appropriate HTTP status code for this violation.
    pub fn status_code(&self) -> StatusCode {
        match self {
            Self::BodyTooLarge { .. } => StatusCode::PAYLOAD_TOO_LARGE,
            Self::RequestTimeout { .. } => StatusCode::GATEWAY_TIMEOUT,
            Self::InvalidContentType { .. } => StatusCode::UNSUPPORTED_MEDIA_TYPE,
            Self::MissingHeader { .. } => StatusCode::BAD_REQUEST,
            Self::JsonTooDeep { .. } => StatusCode::BAD_REQUEST,
            Self::InvalidJson { .. } => StatusCode::BAD_REQUEST,
        }
    }

    /// Returns the error key used in JSON responses.
    pub fn error_key(&self) -> &'static str {
        match self {
            Self::BodyTooLarge { .. } => "body_too_large",
            Self::RequestTimeout { .. } => "request_timeout",
            Self::InvalidContentType { .. } => "invalid_content_type",
            Self::MissingHeader { .. } => "missing_header",
            Self::JsonTooDeep { .. } => "json_too_deep",
            Self::InvalidJson { .. } => "invalid_json",
        }
    }
}

/// Action to take after evaluating a violation through OnViolation policy.
#[derive(Default)]
pub enum ViolationAction {
    /// Reject the request with the default error response.
    #[default]
    Reject,
    /// Let the request through (ignored for Timeout violations).
    Pass,
    /// Respond with a fully custom HTTP response.
    RespondWith(Response<String>),
}

/// Policy for handling violations.
#[derive(Clone, Default)]
pub enum OnViolation {
    /// Return the appropriate 4xx/5xx response immediately.
    #[default]
    Reject,
    /// Log the violation via tracing::warn but forward the request.
    /// Does NOT apply to Timeout violations (no response to forward).
    LogAndPass,
    /// Custom callback. Must be Fn(&Violation) -> ViolationAction + Send + Sync + 'static.
    Custom(Arc<dyn Fn(&Violation) -> ViolationAction + Send + Sync>),
}

impl OnViolation {
    /// Create a custom violation handler from a closure.
    pub fn custom<F>(f: F) -> Self
    where
        F: Fn(&Violation) -> ViolationAction + Send + Sync + 'static,
    {
        Self::Custom(Arc::new(f))
    }
}

impl std::fmt::Debug for OnViolation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Reject => write!(f, "OnViolation::Reject"),
            Self::LogAndPass => write!(f, "OnViolation::LogAndPass"),
            Self::Custom(_) => write!(f, "OnViolation::Custom(...)"),
        }
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib violation::tests`
Expected: All 3 tests PASS

- [ ] **Step 5: Commit**

```bash
git add src/violation.rs
git commit -m "feat: add Violation enum, OnViolation policy, and ViolationAction"
```

---

### Task 3: Response Builder

**Files:**
- Create: `src/response.rs`
- Test: inline `#[cfg(test)]` module

- [ ] **Step 1: Write tests for JSON escaping and response building**

```rust
// src/response.rs

#[cfg(test)]
mod tests {
    use super::*;
    use crate::violation::Violation;
    use http::StatusCode;

    #[test]
    fn escape_json_string_handles_special_chars() {
        assert_eq!(escape_json_string(r#"hello "world""#), r#"hello \"world\""#);
        assert_eq!(escape_json_string("back\\slash"), r#"back\\slash"#);
        assert_eq!(escape_json_string("new\nline"), r#"new\nline"#);
        assert_eq!(escape_json_string("tab\there"), r#"tab\there"#);
    }

    #[test]
    fn escape_json_string_passes_through_clean_input() {
        assert_eq!(escape_json_string("application/json"), "application/json");
        assert_eq!(escape_json_string("Authorization"), "Authorization");
    }

    #[test]
    fn body_too_large_response() {
        let v = Violation::BodyTooLarge { max: 1024, received: 2048 };
        let resp = violation_response(&v);
        assert_eq!(resp.status(), StatusCode::PAYLOAD_TOO_LARGE);
        let body = resp.into_body();
        assert!(body.contains(r#""violation":"body_too_large""#));
        assert!(body.contains(r#""max":1024"#));
        assert!(body.contains(r#""received":2048"#));
    }

    #[test]
    fn missing_header_response() {
        let v = Violation::MissingHeader { header: "Authorization".into() };
        let resp = violation_response(&v);
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let body = resp.into_body();
        assert!(body.contains(r#""violation":"missing_header""#));
        assert!(body.contains(r#""header":"Authorization""#));
    }

    #[test]
    fn invalid_content_type_response() {
        let v = Violation::InvalidContentType {
            received: "text/xml".into(),
            allowed: vec!["application/json".into(), "multipart/form-data".into()],
        };
        let resp = violation_response(&v);
        assert_eq!(resp.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
        let body = resp.into_body();
        assert!(body.contains(r#""violation":"invalid_content_type""#));
        assert!(body.contains(r#""received":"text/xml""#));
        assert!(body.contains(r#""allowed":["application/json","multipart/form-data"]"#));
    }

    #[test]
    fn timeout_response() {
        let v = Violation::RequestTimeout { timeout_ms: 30000 };
        let resp = violation_response(&v);
        assert_eq!(resp.status(), StatusCode::GATEWAY_TIMEOUT);
        let body = resp.into_body();
        assert!(body.contains(r#""violation":"request_timeout""#));
        assert!(body.contains(r#""timeout_ms":30000"#));
    }

    #[test]
    fn json_too_deep_response() {
        let v = Violation::JsonTooDeep { max_depth: 32, found_depth: 128 };
        let resp = violation_response(&v);
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let body = resp.into_body();
        assert!(body.contains(r#""violation":"json_too_deep""#));
        assert!(body.contains(r#""max_depth":32"#));
        assert!(body.contains(r#""found_depth":128"#));
    }

    #[test]
    fn invalid_json_response() {
        let v = Violation::InvalidJson { detail: "unexpected EOF".into() };
        let resp = violation_response(&v);
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let body = resp.into_body();
        assert!(body.contains(r#""violation":"invalid_json""#));
        assert!(body.contains(r#""detail":"unexpected EOF""#));
    }

    #[test]
    fn response_escapes_untrusted_input() {
        let v = Violation::MissingHeader { header: r#"X-Bad"Header"#.into() };
        let resp = violation_response(&v);
        let body = resp.into_body();
        // Verify the JSON is valid by checking escaped quotes
        assert!(body.contains(r#""header":"X-Bad\"Header""#));
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib response::tests`
Expected: FAIL — functions not defined

- [ ] **Step 3: Implement response builder**

```rust
// src/response.rs

use crate::violation::Violation;
use http::{Response, StatusCode};

/// Escape a string for safe inclusion in a JSON string value.
/// Handles quotes, backslashes, and control characters.
pub(crate) fn escape_json_string(s: &str) -> String {
    let mut escaped = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '"' => escaped.push_str(r#"\""#),
            '\\' => escaped.push_str(r#"\\"#),
            '\n' => escaped.push_str(r#"\n"#),
            '\r' => escaped.push_str(r#"\r"#),
            '\t' => escaped.push_str(r#"\t"#),
            c if c.is_control() => {
                escaped.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => escaped.push(c),
        }
    }
    escaped
}

/// Build an HTTP error response for a given violation.
pub fn violation_response(violation: &Violation) -> Response<String> {
    let status = violation.status_code();
    let body = violation_json_body(violation);

    Response::builder()
        .status(status)
        .header("Content-Type", "application/json")
        .body(body)
        .unwrap()
}

fn violation_json_body(violation: &Violation) -> String {
    match violation {
        Violation::BodyTooLarge { max, received } => {
            format!(
                r#"{{"error":"payload too large","violation":"body_too_large","max":{},"received":{}}}"#,
                max, received
            )
        }
        Violation::RequestTimeout { timeout_ms } => {
            format!(
                r#"{{"error":"request timeout","violation":"request_timeout","timeout_ms":{}}}"#,
                timeout_ms
            )
        }
        Violation::InvalidContentType { received, allowed } => {
            let received_escaped = escape_json_string(received);
            let allowed_json: Vec<String> = allowed
                .iter()
                .map(|a| format!(r#""{}""#, escape_json_string(a)))
                .collect();
            format!(
                r#"{{"error":"unsupported content type","violation":"invalid_content_type","received":"{}","allowed":[{}]}}"#,
                received_escaped,
                allowed_json.join(",")
            )
        }
        Violation::MissingHeader { header } => {
            format!(
                r#"{{"error":"missing required header","violation":"missing_header","header":"{}"}}"#,
                escape_json_string(header)
            )
        }
        Violation::JsonTooDeep { max_depth, found_depth } => {
            format!(
                r#"{{"error":"json depth exceeded","violation":"json_too_deep","max_depth":{},"found_depth":{}}}"#,
                max_depth, found_depth
            )
        }
        Violation::InvalidJson { detail } => {
            format!(
                r#"{{"error":"invalid json","violation":"invalid_json","detail":"{}"}}"#,
                escape_json_string(detail)
            )
        }
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib response::tests`
Expected: All 8 tests PASS

- [ ] **Step 5: Commit**

```bash
git add src/response.rs
git commit -m "feat: add violation response builder with JSON escaping"
```

---

## Chunk 2: Validation Logic

### Task 4: Content-Type Matching

**Files:**
- Create: `src/content_type.rs`
- Test: inline `#[cfg(test)]` module

- [ ] **Step 1: Write tests for content-type matching**

```rust
// src/content_type.rs

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_match() {
        let allowed = vec!["application/json".to_string()];
        assert!(matches_content_type("application/json", &allowed));
    }

    #[test]
    fn matches_ignoring_params() {
        let allowed = vec!["application/json".to_string()];
        assert!(matches_content_type("application/json; charset=utf-8", &allowed));
    }

    #[test]
    fn matches_case_insensitive() {
        let allowed = vec!["application/json".to_string()];
        assert!(matches_content_type("Application/JSON", &allowed));
    }

    #[test]
    fn rejects_non_matching() {
        let allowed = vec!["application/json".to_string()];
        assert!(!matches_content_type("text/xml", &allowed));
    }

    #[test]
    fn multiple_allowed() {
        let allowed = vec![
            "application/json".to_string(),
            "multipart/form-data".to_string(),
        ];
        assert!(matches_content_type("multipart/form-data; boundary=abc", &allowed));
        assert!(!matches_content_type("text/plain", &allowed));
    }

    #[test]
    fn empty_allowed_rejects_all() {
        let allowed: Vec<String> = vec![];
        assert!(!matches_content_type("application/json", &allowed));
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib content_type::tests`
Expected: FAIL — function not defined

- [ ] **Step 3: Implement content-type matching**

```rust
// src/content_type.rs

/// Extracts the media type from a Content-Type header value,
/// stripping parameters like charset and boundary.
fn extract_media_type(content_type: &str) -> &str {
    content_type
        .split(';')
        .next()
        .unwrap_or(content_type)
        .trim()
}

/// Check if a Content-Type value matches any of the allowed types.
/// Comparison is case-insensitive and ignores parameters.
pub fn matches_content_type(content_type: &str, allowed: &[String]) -> bool {
    let media_type = extract_media_type(content_type);
    allowed
        .iter()
        .any(|a| a.eq_ignore_ascii_case(media_type))
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib content_type::tests`
Expected: All 6 tests PASS

- [ ] **Step 5: Commit**

```bash
git add src/content_type.rs
git commit -m "feat: add content-type matching with media type extraction"
```

---

### Task 5: Required Headers Validation

**Files:**
- Create: `src/headers.rs`
- Test: inline `#[cfg(test)]` module

- [ ] **Step 1: Write tests for header validation**

```rust
// src/headers.rs

#[cfg(test)]
mod tests {
    use super::*;
    use http::HeaderMap;

    #[test]
    fn all_headers_present() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Bearer token".parse().unwrap());
        headers.insert("x-request-id", "abc-123".parse().unwrap());
        let required = vec!["Authorization".to_string(), "X-Request-Id".to_string()];
        assert_eq!(find_missing_header(&headers, &required), None);
    }

    #[test]
    fn missing_header_detected() {
        let headers = HeaderMap::new();
        let required = vec!["Authorization".to_string()];
        assert_eq!(
            find_missing_header(&headers, &required),
            Some("Authorization".to_string())
        );
    }

    #[test]
    fn case_insensitive_lookup() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Bearer token".parse().unwrap());
        let required = vec!["Authorization".to_string()];
        assert_eq!(find_missing_header(&headers, &required), None);
    }

    #[test]
    fn returns_first_missing() {
        let mut headers = HeaderMap::new();
        headers.insert("x-request-id", "abc".parse().unwrap());
        let required = vec![
            "Authorization".to_string(),
            "X-Request-Id".to_string(),
            "X-Tenant-Id".to_string(),
        ];
        assert_eq!(
            find_missing_header(&headers, &required),
            Some("Authorization".to_string())
        );
    }

    #[test]
    fn empty_required_always_passes() {
        let headers = HeaderMap::new();
        let required: Vec<String> = vec![];
        assert_eq!(find_missing_header(&headers, &required), None);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib headers::tests`
Expected: FAIL — function not defined

- [ ] **Step 3: Implement header validation**

```rust
// src/headers.rs

use http::HeaderMap;

/// Check that all required headers are present.
/// Returns the first missing header name, or None if all present.
/// Lookup is case-insensitive (HeaderMap handles this).
pub fn find_missing_header(headers: &HeaderMap, required: &[String]) -> Option<String> {
    for name in required {
        if headers.get(name.as_str()).is_none() {
            return Some(name.clone());
        }
    }
    None
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib headers::tests`
Expected: All 5 tests PASS

- [ ] **Step 5: Commit**

```bash
git add src/headers.rs
git commit -m "feat: add required headers validation"
```

---

### Task 6: Body Size Checking

**Files:**
- Create: `src/body.rs`
- Test: inline `#[cfg(test)]` module

- [ ] **Step 1: Write tests for Content-Length pre-check**

```rust
// src/body.rs

#[cfg(test)]
mod tests {
    use super::*;
    use http::HeaderMap;

    #[test]
    fn content_length_within_limit() {
        let mut headers = HeaderMap::new();
        headers.insert("content-length", "500".parse().unwrap());
        assert_eq!(check_content_length(&headers, 1024), None);
    }

    #[test]
    fn content_length_exceeds_limit() {
        let mut headers = HeaderMap::new();
        headers.insert("content-length", "2048".parse().unwrap());
        assert_eq!(check_content_length(&headers, 1024), Some(2048));
    }

    #[test]
    fn content_length_exactly_at_limit() {
        let mut headers = HeaderMap::new();
        headers.insert("content-length", "1024".parse().unwrap());
        assert_eq!(check_content_length(&headers, 1024), None);
    }

    #[test]
    fn no_content_length_header() {
        let headers = HeaderMap::new();
        assert_eq!(check_content_length(&headers, 1024), None);
    }

    #[test]
    fn invalid_content_length_value() {
        let mut headers = HeaderMap::new();
        headers.insert("content-length", "not-a-number".parse().unwrap());
        assert_eq!(check_content_length(&headers, 1024), None);
    }

    #[test]
    fn is_bodyless_method_detection() {
        assert!(is_bodyless_method(&http::Method::GET));
        assert!(is_bodyless_method(&http::Method::HEAD));
        assert!(is_bodyless_method(&http::Method::DELETE));
        assert!(is_bodyless_method(&http::Method::OPTIONS));
        assert!(!is_bodyless_method(&http::Method::POST));
        assert!(!is_bodyless_method(&http::Method::PUT));
        assert!(!is_bodyless_method(&http::Method::PATCH));
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib body::tests`
Expected: FAIL — functions not defined

- [ ] **Step 3: Implement body size checking**

```rust
// src/body.rs

use http::{HeaderMap, Method};

/// Check Content-Length header against max body size.
/// Returns Some(received_size) if it exceeds the limit, None otherwise.
/// Returns None if header is absent or unparseable (stream limit handles those).
pub fn check_content_length(headers: &HeaderMap, max_body_size: u64) -> Option<u64> {
    let value = headers.get("content-length")?;
    let length: u64 = value.to_str().ok()?.parse().ok()?;
    if length > max_body_size {
        Some(length)
    } else {
        None
    }
}

/// Returns true for HTTP methods that typically carry no body.
pub fn is_bodyless_method(method: &Method) -> bool {
    matches!(
        *method,
        Method::GET | Method::HEAD | Method::DELETE | Method::OPTIONS
    )
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib body::tests`
Expected: All 7 tests PASS

- [ ] **Step 5: Commit**

```bash
git add src/body.rs
git commit -m "feat: add body size checking and bodyless method detection"
```

---

## Chunk 3: Core Middleware

### Task 7: Guard Config and Builder

**Files:**
- Create: `src/guard.rs`
- Test: inline `#[cfg(test)]` module

- [ ] **Step 1: Write tests for builder**

```rust
// src/guard.rs

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn builder_defaults() {
        let guard = RequestGuard::builder().build();
        assert_eq!(guard.config.max_body_size, None);
        assert_eq!(guard.config.timeout, None);
        assert!(guard.config.allowed_content_types.is_none());
        assert!(guard.config.required_headers.is_empty());
        assert!(matches!(guard.on_violation, OnViolation::Reject));
    }

    #[test]
    fn builder_sets_all_fields() {
        let guard = RequestGuard::builder()
            .max_body_size(1_048_576)
            .timeout(Duration::from_secs(30))
            .allowed_content_types(["application/json"])
            .require_header("Authorization")
            .require_header("X-Request-Id")
            .on_violation(OnViolation::LogAndPass)
            .build();

        assert_eq!(guard.config.max_body_size, Some(1_048_576));
        assert_eq!(guard.config.timeout, Some(Duration::from_secs(30)));
        assert_eq!(
            guard.config.allowed_content_types,
            Some(vec!["application/json".to_string()])
        );
        assert_eq!(
            guard.config.required_headers,
            vec!["Authorization".to_string(), "X-Request-Id".to_string()]
        );
        assert!(matches!(guard.on_violation, OnViolation::LogAndPass));
    }

    #[test]
    fn builder_chaining() {
        // Verify fluent interface compiles and works
        let _guard = RequestGuard::builder()
            .max_body_size(1024)
            .timeout(Duration::from_secs(5))
            .require_header("Auth")
            .build();
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib guard::tests`
Expected: FAIL — types not defined

- [ ] **Step 3: Implement RequestGuard and builder**

```rust
// src/guard.rs

use crate::violation::OnViolation;
use std::time::Duration;

/// Resolved guard configuration (immutable after build).
#[derive(Debug, Clone)]
pub struct GuardConfig {
    pub(crate) max_body_size: Option<u64>,
    pub(crate) timeout: Option<Duration>,
    pub(crate) allowed_content_types: Option<Vec<String>>,
    pub(crate) required_headers: Vec<String>,
    #[cfg(feature = "json")]
    pub(crate) max_json_depth: Option<u32>,
}

/// The built guard holding config and violation policy.
#[derive(Clone)]
pub struct RequestGuard {
    pub(crate) config: GuardConfig,
    pub(crate) on_violation: OnViolation,
}

impl RequestGuard {
    pub fn builder() -> RequestGuardBuilder {
        RequestGuardBuilder::default()
    }

    /// Create a Tower layer from this guard.
    pub fn layer(self) -> crate::layer::RequestGuardLayer {
        crate::layer::RequestGuardLayer::new(self)
    }
}

/// Builder for RequestGuard.
pub struct RequestGuardBuilder {
    max_body_size: Option<u64>,
    timeout: Option<Duration>,
    allowed_content_types: Option<Vec<String>>,
    required_headers: Vec<String>,
    on_violation: OnViolation,
    #[cfg(feature = "json")]
    max_json_depth: Option<u32>,
}

impl Default for RequestGuardBuilder {
    fn default() -> Self {
        Self {
            max_body_size: None,
            timeout: None,
            allowed_content_types: None,
            required_headers: Vec::new(),
            on_violation: OnViolation::default(),
            #[cfg(feature = "json")]
            max_json_depth: None,
        }
    }
}

impl RequestGuardBuilder {
    pub fn max_body_size(mut self, size: u64) -> Self {
        self.max_body_size = Some(size);
        self
    }

    pub fn timeout(mut self, duration: Duration) -> Self {
        self.timeout = Some(duration);
        self
    }

    pub fn allowed_content_types<I, S>(mut self, types: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.allowed_content_types = Some(types.into_iter().map(Into::into).collect());
        self
    }

    pub fn require_header(mut self, name: impl Into<String>) -> Self {
        self.required_headers.push(name.into());
        self
    }

    pub fn on_violation(mut self, policy: OnViolation) -> Self {
        self.on_violation = policy;
        self
    }

    #[cfg(feature = "json")]
    pub fn max_json_depth(mut self, depth: u32) -> Self {
        self.max_json_depth = Some(depth);
        self
    }

    pub fn build(self) -> RequestGuard {
        RequestGuard {
            config: GuardConfig {
                max_body_size: self.max_body_size,
                timeout: self.timeout,
                allowed_content_types: self.allowed_content_types,
                required_headers: self.required_headers,
                #[cfg(feature = "json")]
                max_json_depth: self.max_json_depth,
            },
            on_violation: self.on_violation,
        }
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib guard::tests`
Expected: All 3 tests PASS

- [ ] **Step 5: Commit**

```bash
git add src/guard.rs
git commit -m "feat: add RequestGuard builder with config"
```

---

### Task 8: Route Guard (Per-Route Overrides)

**Files:**
- Create: `src/route.rs`
- Test: inline `#[cfg(test)]` module

- [ ] **Step 1: Write tests for RouteGuardConfig and merging**

```rust
// src/route.rs

#[cfg(test)]
mod tests {
    use super::*;
    use crate::guard::GuardConfig;
    use std::time::Duration;

    fn base_config() -> GuardConfig {
        GuardConfig {
            max_body_size: Some(1024),
            timeout: Some(Duration::from_secs(30)),
            allowed_content_types: Some(vec!["application/json".into()]),
            required_headers: vec!["Authorization".into(), "X-Request-Id".into()],
            #[cfg(feature = "json")]
            max_json_depth: Some(32),
        }
    }

    #[test]
    fn merge_overrides_numeric_values() {
        let route = RouteGuardConfig {
            max_body_size: Some(2048),
            timeout: Some(Duration::from_secs(60)),
            ..Default::default()
        };
        let merged = route.merge_with(&base_config());
        assert_eq!(merged.max_body_size, Some(2048));
        assert_eq!(merged.timeout, Some(Duration::from_secs(60)));
    }

    #[test]
    fn merge_replaces_content_types() {
        let route = RouteGuardConfig {
            allowed_content_types: Some(vec!["multipart/form-data".into()]),
            ..Default::default()
        };
        let merged = route.merge_with(&base_config());
        assert_eq!(
            merged.allowed_content_types,
            Some(vec!["multipart/form-data".into()])
        );
    }

    #[test]
    fn merge_skip_header_removes() {
        let route = RouteGuardConfig {
            skip_headers: vec!["Authorization".into()],
            ..Default::default()
        };
        let merged = route.merge_with(&base_config());
        assert_eq!(merged.required_headers, vec!["X-Request-Id".to_string()]);
    }

    #[test]
    fn merge_require_header_adds() {
        let route = RouteGuardConfig {
            extra_required_headers: vec!["X-Tenant-Id".into()],
            ..Default::default()
        };
        let merged = route.merge_with(&base_config());
        assert!(merged.required_headers.contains(&"X-Tenant-Id".to_string()));
        assert!(merged.required_headers.contains(&"Authorization".to_string()));
    }

    #[test]
    fn merge_skip_all_clears_everything() {
        let route = RouteGuardConfig {
            skip_all: true,
            ..Default::default()
        };
        let merged = route.merge_with(&base_config());
        assert_eq!(merged.max_body_size, None);
        assert_eq!(merged.timeout, None);
        assert!(merged.allowed_content_types.is_none());
        assert!(merged.required_headers.is_empty());
    }

    #[test]
    fn merge_inherits_unset_values() {
        let route = RouteGuardConfig::default();
        let merged = route.merge_with(&base_config());
        assert_eq!(merged.max_body_size, Some(1024));
        assert_eq!(merged.timeout, Some(Duration::from_secs(30)));
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib route::tests`
Expected: FAIL — types not defined

- [ ] **Step 3: Implement RouteGuardConfig, merge_with, and route_guard helper**

```rust
// src/route.rs

use crate::guard::GuardConfig;
use std::time::Duration;
use tower_layer::Layer;
use tower_service::Service;
use http::Request;
use std::task::{Context, Poll};

/// Per-route override configuration. Inserted into request extensions
/// by the `route_guard` layer.
#[derive(Debug, Clone, Default)]
pub struct RouteGuardConfig {
    pub(crate) max_body_size: Option<u64>,
    pub(crate) timeout: Option<Duration>,
    pub(crate) allowed_content_types: Option<Vec<String>>,
    pub(crate) skip_headers: Vec<String>,
    pub(crate) extra_required_headers: Vec<String>,
    pub(crate) skip_all: bool,
    #[cfg(feature = "json")]
    pub(crate) max_json_depth: Option<u32>,
}

impl RouteGuardConfig {
    pub fn max_body_size(mut self, size: u64) -> Self {
        self.max_body_size = Some(size);
        self
    }

    pub fn timeout(mut self, duration: Duration) -> Self {
        self.timeout = Some(duration);
        self
    }

    pub fn allowed_content_types<I, S>(mut self, types: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.allowed_content_types = Some(types.into_iter().map(Into::into).collect());
        self
    }

    pub fn skip_header(mut self, name: impl Into<String>) -> Self {
        self.skip_headers.push(name.into());
        self
    }

    pub fn require_header(mut self, name: impl Into<String>) -> Self {
        self.extra_required_headers.push(name.into());
        self
    }

    pub fn skip_all(mut self) -> Self {
        self.skip_all = true;
        self
    }

    #[cfg(feature = "json")]
    pub fn max_json_depth(mut self, depth: u32) -> Self {
        self.max_json_depth = Some(depth);
        self
    }

    /// Merge this route config with the global config.
    /// Route values override globals; unset values inherit from global.
    pub fn merge_with(&self, global: &GuardConfig) -> GuardConfig {
        if self.skip_all {
            return GuardConfig {
                max_body_size: None,
                timeout: None,
                allowed_content_types: None,
                required_headers: Vec::new(),
                #[cfg(feature = "json")]
                max_json_depth: None,
            };
        }

        // Required headers: start with global, remove skipped, add extras
        let mut required_headers = global.required_headers.clone();
        required_headers.retain(|h| {
            !self.skip_headers.iter().any(|s| s.eq_ignore_ascii_case(h))
        });
        for extra in &self.extra_required_headers {
            if !required_headers.iter().any(|h| h.eq_ignore_ascii_case(extra)) {
                required_headers.push(extra.clone());
            }
        }

        GuardConfig {
            max_body_size: self.max_body_size.or(global.max_body_size),
            timeout: self.timeout.or(global.timeout),
            allowed_content_types: self
                .allowed_content_types
                .clone()
                .or_else(|| global.allowed_content_types.clone()),
            required_headers,
            #[cfg(feature = "json")]
            max_json_depth: self.max_json_depth.or(global.max_json_depth),
        }
    }
}

/// Create a per-route guard override layer.
/// The closure receives a `RouteGuardConfig` to configure route-specific overrides.
pub fn route_guard<F>(f: F) -> RouteGuardLayer
where
    F: FnOnce(RouteGuardConfig) -> RouteGuardConfig,
{
    RouteGuardLayer(f(RouteGuardConfig::default()))
}

/// Layer that inserts RouteGuardConfig into request extensions.
#[derive(Debug, Clone)]
pub struct RouteGuardLayer(RouteGuardConfig);

impl<S> Layer<S> for RouteGuardLayer {
    type Service = RouteGuardService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        RouteGuardService {
            inner,
            config: self.0.clone(),
        }
    }
}

/// Service that inserts RouteGuardConfig into request extensions.
#[derive(Debug, Clone)]
pub struct RouteGuardService<S> {
    inner: S,
    config: RouteGuardConfig,
}

impl<S, B> Service<Request<B>> for RouteGuardService<S>
where
    S: Service<Request<B>>,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = S::Future;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, mut req: Request<B>) -> Self::Future {
        req.extensions_mut().insert(self.config.clone());
        self.inner.call(req)
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib route::tests`
Expected: All 6 tests PASS

- [ ] **Step 5: Commit**

```bash
git add src/route.rs
git commit -m "feat: add route_guard per-route overrides with config merging"
```

---

### Task 9: Layer and Service Stub

**Files:**
- Create: `src/layer.rs`
- Modify: `src/service.rs` (add struct stub)

- [ ] **Step 1: Add RequestGuardService struct stub to service.rs**

This stub is needed so `layer.rs` can reference the type. The full implementation comes in Task 10.

```rust
// src/service.rs (minimal stub)

use crate::guard::RequestGuard;
use std::sync::Arc;

/// Tower Service that validates requests before forwarding.
pub struct RequestGuardService<S> {
    pub(crate) inner: S,
    pub(crate) guard: Arc<RequestGuard>,
}
```

- [ ] **Step 2: Implement RequestGuardLayer**

```rust
// src/layer.rs

use crate::guard::RequestGuard;
use crate::service::RequestGuardService;
use tower_layer::Layer;
use std::sync::Arc;

/// Tower Layer that applies request validation.
#[derive(Clone)]
pub struct RequestGuardLayer {
    pub(crate) guard: Arc<RequestGuard>,
}

impl RequestGuardLayer {
    pub fn new(guard: RequestGuard) -> Self {
        Self {
            guard: Arc::new(guard),
        }
    }
}

impl<S> Layer<S> for RequestGuardLayer {
    type Service = RequestGuardService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        RequestGuardService {
            inner,
            guard: self.guard.clone(),
        }
    }
}
```

- [ ] **Step 3: Run cargo check**

Run: `cargo check`
Expected: Compiles — layer references the service stub

- [ ] **Step 4: Commit**

```bash
git add src/layer.rs src/service.rs
git commit -m "feat: add RequestGuardLayer and RequestGuardService stub"
```

---

### Task 10: Service (Core Orchestration)

**Files:**
- Create: `src/service.rs`
- Test: `tests/integration.rs`

This is the main orchestration service. It ties together all validation modules. The struct stub was created in Task 9 — this task adds the full `Service` trait implementation.

- [ ] **Step 1: Write integration tests first (TDD)**

Write `tests/integration.rs` (shown in Step 3 below) BEFORE the implementation. Run them to see them fail:

Run: `cargo test --test integration 2>&1 || true`
Expected: FAIL — `Service` not implemented for `RequestGuardService`

- [ ] **Step 2: Implement RequestGuardService**

```rust
// src/service.rs

use crate::body::{check_content_length, is_bodyless_method};
use crate::content_type::matches_content_type;
use crate::guard::{GuardConfig, RequestGuard};
use crate::headers::find_missing_header;
use crate::response::violation_response;
use crate::route::RouteGuardConfig;
use crate::violation::{OnViolation, Violation, ViolationAction};
use http::{Request, Response};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tower_service::Service;

/// Tower Service that validates requests before forwarding.
pub struct RequestGuardService<S> {
    pub(crate) inner: S,
    pub(crate) guard: Arc<RequestGuard>,
}

impl<S: Clone> Clone for RequestGuardService<S> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            guard: self.guard.clone(),
        }
    }
}

impl<S, B, ResBody> Service<Request<B>> for RequestGuardService<S>
where
    S: Service<Request<B>, Response = Response<ResBody>> + Clone + Send + 'static,
    S::Future: Send,
    S::Error: Send,
    B: Send + 'static,
    ResBody: From<String> + Send,
{
    type Response = Response<ResBody>;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<B>) -> Self::Future {
        let guard = self.guard.clone();
        let mut inner = self.inner.clone();
        std::mem::swap(&mut self.inner, &mut inner);

        Box::pin(async move {
            // Resolve effective config (merge route overrides)
            let effective = match req.extensions().get::<RouteGuardConfig>() {
                Some(route_config) => route_config.merge_with(&guard.config),
                None => guard.config.clone(),
            };

            let is_bodyless = is_bodyless_method(req.method());

            // 1. Content-Type check (skip for bodyless methods)
            if !is_bodyless {
                if let Some(ref allowed) = effective.allowed_content_types {
                    let content_type = req
                        .headers()
                        .get("content-type")
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("");
                    if !matches_content_type(content_type, allowed) {
                        let violation = Violation::InvalidContentType {
                            received: content_type.to_string(),
                            allowed: allowed.clone(),
                        };
                        if let Some(resp) = handle_violation(&violation, &guard.on_violation) {
                            return Ok(resp.map(Into::into));
                        }
                    }
                }
            }

            // 2. Required headers check
            if !effective.required_headers.is_empty() {
                if let Some(missing) =
                    find_missing_header(req.headers(), &effective.required_headers)
                {
                    let violation = Violation::MissingHeader { header: missing };
                    if let Some(resp) = handle_violation(&violation, &guard.on_violation) {
                        return Ok(resp.map(Into::into));
                    }
                }
            }

            // 3. Content-Length pre-check (skip for bodyless methods)
            if !is_bodyless {
                if let Some(max) = effective.max_body_size {
                    if let Some(received) = check_content_length(req.headers(), max) {
                        let violation = Violation::BodyTooLarge {
                            max,
                            received,
                        };
                        if let Some(resp) = handle_violation(&violation, &guard.on_violation) {
                            return Ok(resp.map(Into::into));
                        }
                    }
                }
            }

            // Steps 4-5 (JSON depth + full body buffering with size check)
            // are handled by BufferedRequestGuardService when json feature is enabled.
            //
            // Body size enforcement strategy for non-buffered path:
            // - Content-Length present: rejected in step 3 above (O(1), no body read)
            // - Content-Length absent (chunked): not enforced here (would require
            //   changing the body type to LimitedBody<B>, breaking the generic service
            //   contract). For full stream limiting of chunked bodies, enable the
            //   `json` feature (which buffers and checks) or combine with
            //   tower-http::RequestBodyLimitLayer.

            // 6. Timeout wrap
            if let Some(timeout_duration) = effective.timeout {
                match tokio::time::timeout(timeout_duration, inner.call(req)).await {
                    Ok(result) => result,
                    Err(_elapsed) => {
                        let violation = Violation::RequestTimeout {
                            timeout_ms: timeout_duration.as_millis() as u64,
                        };
                        // Timeout always produces a response — LogAndPass is ignored
                        let resp = handle_timeout_violation(&violation, &guard.on_violation);
                        Ok(resp.map(Into::into))
                    }
                }
            } else {
                inner.call(req).await
            }
        })
    }
}

/// Handle a pre-handler violation according to the OnViolation policy.
/// Returns Some(response) if the request should be rejected, None if it should pass.
pub(crate) fn handle_violation(violation: &Violation, policy: &OnViolation) -> Option<Response<String>> {
    match policy {
        OnViolation::Reject => Some(violation_response(violation)),
        OnViolation::LogAndPass => {
            tracing::warn!(?violation, "request guard violation (log-and-pass)");
            None
        }
        OnViolation::Custom(callback) => match callback(violation) {
            ViolationAction::Reject => Some(violation_response(violation)),
            ViolationAction::Pass => None,
            ViolationAction::RespondWith(resp) => Some(resp),
        },
    }
}

/// Handle a timeout violation. LogAndPass is ignored for timeouts.
pub(crate) fn handle_timeout_violation(violation: &Violation, policy: &OnViolation) -> Response<String> {
    match policy {
        OnViolation::Custom(callback) => match callback(violation) {
            ViolationAction::RespondWith(resp) => resp,
            _ => violation_response(violation), // Reject or Pass both produce default response
        },
        _ => violation_response(violation),
    }
}
```

- [ ] **Step 3: Integration test code (write this BEFORE Step 2, see Step 1)**

```rust
// tests/integration.rs

use http::{Request, Response, StatusCode};
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;
use tower_layer::Layer;
use tower_service::Service;

use tower_request_guard::{OnViolation, RequestGuard, Violation, ViolationAction};

// ── Test helper service ──────────────────────────────────────────────

#[derive(Clone)]
struct OkService;

impl Service<Request<String>> for OkService {
    type Response = Response<String>;
    type Error = std::convert::Infallible;
    type Future = std::future::Ready<Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, _req: Request<String>) -> Self::Future {
        std::future::ready(Ok(Response::new("ok".to_string())))
    }
}

#[derive(Clone)]
struct SlowService;

impl Service<Request<String>> for SlowService {
    type Response = Response<String>;
    type Error = std::convert::Infallible;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, _req: Request<String>) -> Self::Future {
        Box::pin(async {
            tokio::time::sleep(Duration::from_secs(10)).await;
            Ok(Response::new("slow".to_string()))
        })
    }
}

// ── Helper builders ──────────────────────────────────────────────────

fn json_post(body: &str) -> Request<String> {
    Request::builder()
        .method("POST")
        .uri("/api/test")
        .header("content-type", "application/json")
        .header("authorization", "Bearer token")
        .body(body.to_string())
        .unwrap()
}

fn get_request() -> Request<String> {
    Request::builder()
        .method("GET")
        .uri("/api/test")
        .header("authorization", "Bearer token")
        .body(String::new())
        .unwrap()
}

// ── Tests ────────────────────────────────────────────────────────────

#[tokio::test]
async fn valid_request_passes_through() {
    let guard = RequestGuard::builder()
        .allowed_content_types(["application/json"])
        .require_header("Authorization")
        .build();

    let mut svc = guard.layer().layer(OkService);
    let resp = svc.call(json_post("{}")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn rejects_wrong_content_type() {
    let guard = RequestGuard::builder()
        .allowed_content_types(["application/json"])
        .build();

    let mut svc = guard.layer().layer(OkService);
    let req = Request::builder()
        .method("POST")
        .uri("/test")
        .header("content-type", "text/xml")
        .body(String::new())
        .unwrap();

    let resp = svc.call(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
    let body = resp.into_body();
    assert!(body.contains("invalid_content_type"));
}

#[tokio::test]
async fn rejects_missing_required_header() {
    let guard = RequestGuard::builder()
        .require_header("Authorization")
        .build();

    let mut svc = guard.layer().layer(OkService);
    let req = Request::builder()
        .method("POST")
        .uri("/test")
        .body(String::new())
        .unwrap();

    let resp = svc.call(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let body = resp.into_body();
    assert!(body.contains("missing_header"));
    assert!(body.contains("Authorization"));
}

#[tokio::test]
async fn rejects_body_too_large_via_content_length() {
    let guard = RequestGuard::builder()
        .max_body_size(100)
        .build();

    let mut svc = guard.layer().layer(OkService);
    let req = Request::builder()
        .method("POST")
        .uri("/test")
        .header("content-length", "500")
        .body(String::new())
        .unwrap();

    let resp = svc.call(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::PAYLOAD_TOO_LARGE);
    let body = resp.into_body();
    assert!(body.contains("body_too_large"));
}

#[tokio::test]
async fn skips_body_checks_for_get_requests() {
    let guard = RequestGuard::builder()
        .max_body_size(10)
        .allowed_content_types(["application/json"])
        .require_header("Authorization")
        .build();

    let mut svc = guard.layer().layer(OkService);
    // GET with no content-type, no body — should pass
    let resp = svc.call(get_request()).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn timeout_returns_504() {
    let guard = RequestGuard::builder()
        .timeout(Duration::from_millis(50))
        .build();

    let mut svc = guard.layer().layer(SlowService);
    let req = Request::builder()
        .method("GET")
        .uri("/test")
        .body(String::new())
        .unwrap();

    let resp = svc.call(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::GATEWAY_TIMEOUT);
    let body = resp.into_body();
    assert!(body.contains("request_timeout"));
}

#[tokio::test]
async fn log_and_pass_forwards_invalid_request() {
    let guard = RequestGuard::builder()
        .require_header("Authorization")
        .on_violation(OnViolation::LogAndPass)
        .build();

    let mut svc = guard.layer().layer(OkService);
    let req = Request::builder()
        .method("POST")
        .uri("/test")
        .body(String::new())
        .unwrap();

    // Missing Authorization but LogAndPass lets it through
    let resp = svc.call(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn custom_violation_handler() {
    let guard = RequestGuard::builder()
        .require_header("Authorization")
        .on_violation(OnViolation::custom(|_violation| {
            let resp = Response::builder()
                .status(StatusCode::FORBIDDEN)
                .body(r#"{"custom":"error"}"#.to_string())
                .unwrap();
            ViolationAction::RespondWith(resp)
        }))
        .build();

    let mut svc = guard.layer().layer(OkService);
    let req = Request::builder()
        .method("POST")
        .uri("/test")
        .body(String::new())
        .unwrap();

    let resp = svc.call(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    assert!(resp.into_body().contains("custom"));
}

#[tokio::test]
async fn content_type_matches_with_charset() {
    let guard = RequestGuard::builder()
        .allowed_content_types(["application/json"])
        .build();

    let mut svc = guard.layer().layer(OkService);
    let req = Request::builder()
        .method("POST")
        .uri("/test")
        .header("content-type", "application/json; charset=utf-8")
        .body(String::new())
        .unwrap();

    let resp = svc.call(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}
```

- [ ] **Step 4: Run integration tests**

Run: `cargo test --test integration`
Expected: All tests PASS

- [ ] **Step 5: Commit**

```bash
git add src/service.rs tests/integration.rs
git commit -m "feat: add RequestGuardService with full validation flow"
```

---

### Task 11: Route Guard Integration Tests

**Files:**
- Modify: `tests/integration.rs`

- [ ] **Step 1: Add route_guard integration tests**

Append to `tests/integration.rs`:

```rust
use tower_request_guard::route_guard;

#[tokio::test]
async fn route_guard_overrides_body_size() {
    let guard = RequestGuard::builder()
        .max_body_size(100)
        .build();

    // Route allows larger bodies
    let route_layer = route_guard(|r| r.max_body_size(10_000));

    let mut svc = guard.layer().layer(route_layer.layer(OkService));
    let req = Request::builder()
        .method("POST")
        .uri("/test")
        .header("content-length", "5000")
        .body(String::new())
        .unwrap();

    let resp = svc.call(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn route_guard_skip_all_bypasses_everything() {
    let guard = RequestGuard::builder()
        .max_body_size(10)
        .require_header("Authorization")
        .allowed_content_types(["application/json"])
        .build();

    let route_layer = route_guard(|r| r.skip_all());
    let mut svc = guard.layer().layer(route_layer.layer(OkService));

    // No auth, wrong content-type, large body — all skipped
    let req = Request::builder()
        .method("POST")
        .uri("/test")
        .header("content-type", "text/xml")
        .header("content-length", "999999")
        .body(String::new())
        .unwrap();

    let resp = svc.call(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn route_guard_skip_header() {
    let guard = RequestGuard::builder()
        .require_header("Authorization")
        .require_header("X-Request-Id")
        .build();

    let route_layer = route_guard(|r| r.skip_header("Authorization"));
    let mut svc = guard.layer().layer(route_layer.layer(OkService));

    // Only X-Request-Id required, Authorization skipped
    let req = Request::builder()
        .method("POST")
        .uri("/test")
        .header("x-request-id", "abc")
        .body(String::new())
        .unwrap();

    let resp = svc.call(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}
```

- [ ] **Step 2: Run integration tests**

Run: `cargo test --test integration`
Expected: All tests PASS (9 original + 3 new)

- [ ] **Step 3: Commit**

```bash
git add tests/integration.rs
git commit -m "test: add route_guard integration tests"
```

---

## Chunk 4: JSON Feature

### Task 12: JSON Depth Validation

**Files:**
- Create: `src/json.rs`
- Test: inline `#[cfg(test)]` module
- Test: `tests/json_tests.rs`

- [ ] **Step 1: Write tests for JSON depth checking**

```rust
// src/json.rs

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flat_json_passes() {
        assert_eq!(check_json_depth(b"{\"a\":1,\"b\":2}", 32), Ok(1));
    }

    #[test]
    fn nested_within_limit() {
        assert_eq!(check_json_depth(b"{\"a\":{\"b\":{\"c\":1}}}", 5), Ok(3));
    }

    #[test]
    fn nested_exceeds_limit() {
        let result = check_json_depth(b"{\"a\":{\"b\":{\"c\":1}}}", 2);
        assert!(matches!(result, Err(JsonDepthError::TooDeep { found_depth: 3, max_depth: 2 })));
    }

    #[test]
    fn array_nesting_counts() {
        // [[[]]] = depth 3
        assert_eq!(check_json_depth(b"[[[]]]", 5), Ok(3));
    }

    #[test]
    fn mixed_nesting() {
        // {"a":[{"b":1}]} = depth 3 (object -> array -> object)
        assert_eq!(check_json_depth(br#"{"a":[{"b":1}]}"#, 5), Ok(3));
    }

    #[test]
    fn empty_object() {
        assert_eq!(check_json_depth(b"{}", 32), Ok(1));
    }

    #[test]
    fn empty_array() {
        assert_eq!(check_json_depth(b"[]", 32), Ok(1));
    }

    #[test]
    fn scalar_value() {
        assert_eq!(check_json_depth(b"42", 32), Ok(0));
        assert_eq!(check_json_depth(b"\"hello\"", 32), Ok(0));
        assert_eq!(check_json_depth(b"true", 32), Ok(0));
        assert_eq!(check_json_depth(b"null", 32), Ok(0));
    }

    #[test]
    fn malformed_json() {
        let result = check_json_depth(b"{invalid", 32);
        assert!(matches!(result, Err(JsonDepthError::Malformed { .. })));
    }

    #[test]
    fn deeply_nested_bomb() {
        // 100 levels of nesting
        let open: String = "{\"a\":".repeat(100);
        let close: String = "}".repeat(100);
        let bomb = format!("{}1{}", open, close);
        let result = check_json_depth(bomb.as_bytes(), 32);
        assert!(matches!(result, Err(JsonDepthError::TooDeep { .. })));
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib json::tests --features json`
Expected: FAIL — types not defined

- [ ] **Step 3: Implement JSON depth checker**

```rust
// src/json.rs

/// Errors from JSON depth checking.
#[derive(Debug, PartialEq)]
pub enum JsonDepthError {
    TooDeep { max_depth: u32, found_depth: u32 },
    Malformed { detail: String },
}

/// Check the maximum nesting depth of a JSON value.
/// Returns Ok(depth) if within limit, Err otherwise.
///
/// Uses serde_json::Value to parse and then walks the tree.
/// This is robust against all JSON edge cases.
pub fn check_json_depth(data: &[u8], max_depth: u32) -> Result<u32, JsonDepthError> {
    let value: serde_json::Value = serde_json::from_slice(data).map_err(|e| {
        JsonDepthError::Malformed {
            detail: e.to_string(),
        }
    })?;

    let depth = measure_depth(&value);
    if depth > max_depth {
        Err(JsonDepthError::TooDeep {
            max_depth,
            found_depth: depth,
        })
    } else {
        Ok(depth)
    }
}

fn measure_depth(value: &serde_json::Value) -> u32 {
    match value {
        serde_json::Value::Object(map) => {
            let max_child = map.values().map(measure_depth).max().unwrap_or(0);
            1 + max_child
        }
        serde_json::Value::Array(arr) => {
            let max_child = arr.iter().map(measure_depth).max().unwrap_or(0);
            1 + max_child
        }
        _ => 0,
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib json::tests --features json`
Expected: All 11 tests PASS

- [ ] **Step 5: Commit**

```bash
git add src/json.rs
git commit -m "feat: add JSON depth validation with serde_json"
```

---

### Task 13: Buffered Service Variant

**Files:**
- Create: `src/buffered.rs`
- Test: `tests/json_tests.rs`

- [ ] **Step 1: Implement BufferedRequestGuardLayer and BufferedRequestGuardService**

```rust
// src/buffered.rs

use crate::body::{check_content_length, is_bodyless_method};
use crate::content_type::matches_content_type;
use crate::guard::RequestGuard;
use crate::headers::find_missing_header;
use crate::json::{check_json_depth, JsonDepthError};
use crate::response::violation_response;
use crate::route::RouteGuardConfig;
use crate::service::{handle_violation, handle_timeout_violation};
use crate::violation::Violation;
use bytes::Bytes;
use http::{Request, Response};
use http_body_util::BodyExt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tower_layer::Layer;
use tower_service::Service;

/// Tower Layer for the buffered variant (json feature enabled).
#[derive(Clone)]
pub struct BufferedRequestGuardLayer {
    pub(crate) guard: Arc<RequestGuard>,
}

impl BufferedRequestGuardLayer {
    pub fn new(guard: RequestGuard) -> Self {
        Self {
            guard: Arc::new(guard),
        }
    }
}

impl<S> Layer<S> for BufferedRequestGuardLayer {
    type Service = BufferedRequestGuardService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        BufferedRequestGuardService {
            inner,
            guard: self.guard.clone(),
        }
    }
}

/// Tower Service that buffers the body for JSON depth validation.
pub struct BufferedRequestGuardService<S> {
    pub(crate) inner: S,
    pub(crate) guard: Arc<RequestGuard>,
}

impl<S: Clone> Clone for BufferedRequestGuardService<S> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            guard: self.guard.clone(),
        }
    }
}

impl<S, B, ResBody> Service<Request<B>> for BufferedRequestGuardService<S>
where
    S: Service<Request<http_body_util::Full<Bytes>>, Response = Response<ResBody>>
        + Clone
        + Send
        + 'static,
    S::Future: Send,
    S::Error: Send,
    B: http_body::Body<Data = Bytes> + Send + 'static,
    B::Error: std::fmt::Display,
    ResBody: From<String> + Send,
{
    type Response = Response<ResBody>;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<B>) -> Self::Future {
        let guard = self.guard.clone();
        let mut inner = self.inner.clone();
        std::mem::swap(&mut self.inner, &mut inner);

        Box::pin(async move {
            let effective = match req.extensions().get::<RouteGuardConfig>() {
                Some(route_config) => route_config.merge_with(&guard.config),
                None => guard.config.clone(),
            };

            let is_bodyless = is_bodyless_method(req.method());

            // 1. Content-Type check
            if !is_bodyless {
                if let Some(ref allowed) = effective.allowed_content_types {
                    let content_type = req
                        .headers()
                        .get("content-type")
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("");
                    if !matches_content_type(content_type, allowed) {
                        let violation = Violation::InvalidContentType {
                            received: content_type.to_string(),
                            allowed: allowed.clone(),
                        };
                        if let Some(resp) = handle_violation(&violation, &guard.on_violation) {
                            return Ok(resp.map(Into::into));
                        }
                    }
                }
            }

            // 2. Required headers check
            if !effective.required_headers.is_empty() {
                if let Some(missing) =
                    find_missing_header(req.headers(), &effective.required_headers)
                {
                    let violation = Violation::MissingHeader { header: missing };
                    if let Some(resp) = handle_violation(&violation, &guard.on_violation) {
                        return Ok(resp.map(Into::into));
                    }
                }
            }

            // 3. Content-Length pre-check
            if !is_bodyless {
                if let Some(max) = effective.max_body_size {
                    if let Some(received) = check_content_length(req.headers(), max) {
                        let violation = Violation::BodyTooLarge { max, received };
                        if let Some(resp) = handle_violation(&violation, &guard.on_violation) {
                            return Ok(resp.map(Into::into));
                        }
                    }
                }
            }

            // 4. Buffer body and check JSON depth
            let (parts, body) = req.into_parts();
            let body_bytes = match body.collect().await {
                Ok(collected) => collected.to_bytes(),
                Err(e) => {
                    let violation = Violation::InvalidJson {
                        detail: e.to_string(),
                    };
                    if let Some(resp) = handle_violation(&violation, &guard.on_violation) {
                        return Ok(resp.map(Into::into));
                    }
                    Bytes::new()
                }
            };

            // Check body size (for chunked transfers without Content-Length)
            if !is_bodyless {
                if let Some(max) = effective.max_body_size {
                    if body_bytes.len() as u64 > max {
                        let violation = Violation::BodyTooLarge {
                            max,
                            received: body_bytes.len() as u64,
                        };
                        if let Some(resp) = handle_violation(&violation, &guard.on_violation) {
                            return Ok(resp.map(Into::into));
                        }
                    }
                }
            }

            // JSON depth check (only for JSON content-type and non-empty body)
            if !is_bodyless && !body_bytes.is_empty() {
                if let Some(max_depth) = effective.max_json_depth {
                    let is_json = parts
                        .headers
                        .get("content-type")
                        .and_then(|v| v.to_str().ok())
                        .map(|ct| ct.to_ascii_lowercase().contains("application/json"))
                        .unwrap_or(false);

                    if is_json {
                        match check_json_depth(&body_bytes, max_depth) {
                            Ok(_) => {}
                            Err(JsonDepthError::TooDeep {
                                max_depth,
                                found_depth,
                            }) => {
                                let violation = Violation::JsonTooDeep {
                                    max_depth,
                                    found_depth,
                                };
                                if let Some(resp) =
                                    handle_violation(&violation, &guard.on_violation)
                                {
                                    return Ok(resp.map(Into::into));
                                }
                            }
                            Err(JsonDepthError::Malformed { detail }) => {
                                let violation = Violation::InvalidJson { detail };
                                if let Some(resp) =
                                    handle_violation(&violation, &guard.on_violation)
                                {
                                    return Ok(resp.map(Into::into));
                                }
                            }
                        }
                    }
                }
            }

            // Reconstruct request with Full<Bytes> body
            let new_req = Request::from_parts(parts, http_body_util::Full::new(body_bytes));

            // 6. Timeout wrap
            if let Some(timeout_duration) = effective.timeout {
                match tokio::time::timeout(timeout_duration, inner.call(new_req)).await {
                    Ok(result) => result,
                    Err(_elapsed) => {
                        let violation = Violation::RequestTimeout {
                            timeout_ms: timeout_duration.as_millis() as u64,
                        };
                        Ok(handle_timeout_violation(&violation, &guard.on_violation)
                            .map(Into::into))
                    }
                }
            } else {
                inner.call(new_req).await
            }
        })
    }
}
```

- [ ] **Step 2: Write JSON integration tests**

```rust
// tests/json_tests.rs

use bytes::Bytes;
use http::{Request, Response, StatusCode};
use http_body_util::Full;
use std::task::{Context, Poll};
use tower_layer::Layer;
use tower_service::Service;

use tower_request_guard::{BufferedRequestGuardLayer, OnViolation, RequestGuard};

#[derive(Clone)]
struct OkService;

impl Service<Request<Full<Bytes>>> for OkService {
    type Response = Response<String>;
    type Error = std::convert::Infallible;
    type Future = std::future::Ready<Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, _req: Request<Full<Bytes>>) -> Self::Future {
        std::future::ready(Ok(Response::new("ok".to_string())))
    }
}

fn json_request(body: &str) -> Request<Full<Bytes>> {
    Request::builder()
        .method("POST")
        .uri("/test")
        .header("content-type", "application/json")
        .body(Full::new(Bytes::from(body.to_string())))
        .unwrap()
}

#[tokio::test]
async fn json_within_depth_limit_passes() {
    let guard = RequestGuard::builder()
        .max_json_depth(10)
        .build();

    let layer = BufferedRequestGuardLayer::new(guard);
    let mut svc = layer.layer(OkService);

    let resp = svc.call(json_request(r#"{"a":{"b":1}}"#)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn json_exceeding_depth_limit_rejected() {
    let guard = RequestGuard::builder()
        .max_json_depth(2)
        .build();

    let layer = BufferedRequestGuardLayer::new(guard);
    let mut svc = layer.layer(OkService);

    let resp = svc
        .call(json_request(r#"{"a":{"b":{"c":1}}}"#))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let body = resp.into_body();
    assert!(body.contains("json_too_deep"));
}

#[tokio::test]
async fn malformed_json_rejected() {
    let guard = RequestGuard::builder()
        .max_json_depth(32)
        .build();

    let layer = BufferedRequestGuardLayer::new(guard);
    let mut svc = layer.layer(OkService);

    let resp = svc.call(json_request("{invalid")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let body = resp.into_body();
    assert!(body.contains("invalid_json"));
}

#[tokio::test]
async fn non_json_content_type_skips_depth_check() {
    let guard = RequestGuard::builder()
        .max_json_depth(1)
        .build();

    let layer = BufferedRequestGuardLayer::new(guard);
    let mut svc = layer.layer(OkService);

    let req = Request::builder()
        .method("POST")
        .uri("/test")
        .header("content-type", "text/plain")
        .body(Full::new(Bytes::from(r#"{"a":{"b":{"c":1}}}"#)))
        .unwrap();

    // Not JSON content-type, depth check skipped
    let resp = svc.call(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn buffered_body_size_check_for_chunked() {
    let guard = RequestGuard::builder()
        .max_body_size(10)
        .max_json_depth(32)
        .build();

    let layer = BufferedRequestGuardLayer::new(guard);
    let mut svc = layer.layer(OkService);

    // No Content-Length header, body exceeds limit after buffering
    let req = Request::builder()
        .method("POST")
        .uri("/test")
        .header("content-type", "application/json")
        .body(Full::new(Bytes::from(r#"{"large":"this body is definitely more than 10 bytes"}"#)))
        .unwrap();

    let resp = svc.call(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::PAYLOAD_TOO_LARGE);
}
```

- [ ] **Step 4: Run JSON tests**

Run: `cargo test --test json_tests --features json`
Expected: All 5 tests PASS

- [ ] **Step 5: Commit**

```bash
git add src/buffered.rs tests/json_tests.rs
git commit -m "feat: add buffered service variant with JSON depth validation"
```

---

## Chunk 5: Integration & Polish

### Task 14: Examples

**Files:**
- Create: `examples/axum_basic.rs`
- Create: `examples/axum_routes.rs`
- Create: `examples/axum_migration.rs`

- [ ] **Step 1: Create axum_basic example**

```rust
// examples/axum_basic.rs

//! Basic request guard setup with Axum.
//!
//! Run: `cargo run --example axum_basic`
//!
//! Test:
//!   curl -X POST -H "Content-Type: application/json" \
//!        -H "Authorization: Bearer token" \
//!        -d '{"hello":"world"}' \
//!        http://localhost:3000/api/users

use axum::{routing::{get, post}, Router};
use std::time::Duration;
use tower_request_guard::RequestGuard;

#[tokio::main]
async fn main() {
    let guard = RequestGuard::builder()
        .max_body_size(1_048_576)                      // 1 MB
        .timeout(Duration::from_secs(30))
        .allowed_content_types(["application/json"])
        .require_header("Authorization")
        .build();

    let app = Router::new()
        .route("/api/users", get(list_users).post(create_user))
        .route("/api/health", get(health))
        .layer(guard.layer());

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    println!("Listening on http://localhost:3000");
    axum::serve(listener, app).await.unwrap();
}

async fn list_users() -> &'static str {
    r#"{"users":[]}"#
}

async fn create_user() -> &'static str {
    r#"{"created":true}"#
}

async fn health() -> &'static str {
    r#"{"status":"ok"}"#
}
```

- [ ] **Step 2: Create axum_routes example**

```rust
// examples/axum_routes.rs

//! Per-route overrides with route_guard.
//!
//! Run: `cargo run --example axum_routes`
//!
//! Test:
//!   # Normal route (1 MB limit, 30s timeout)
//!   curl -X POST -H "Content-Type: application/json" \
//!        -H "Authorization: Bearer token" \
//!        -d '{}' http://localhost:3000/api/users
//!
//!   # Upload route (10 MB limit, 120s timeout, multipart)
//!   curl -X POST -H "Content-Type: multipart/form-data" \
//!        http://localhost:3000/api/upload
//!
//!   # Health (no validations)
//!   curl http://localhost:3000/api/health

use axum::{routing::{get, post}, Router};
use std::time::Duration;
use tower_request_guard::{route_guard, RequestGuard};

#[tokio::main]
async fn main() {
    let guard = RequestGuard::builder()
        .max_body_size(1_048_576)
        .timeout(Duration::from_secs(30))
        .allowed_content_types(["application/json"])
        .require_header("Authorization")
        .build();

    let app = Router::new()
        .route("/api/users", post(create_user))
        .route(
            "/api/upload",
            post(upload).layer(route_guard(|r| {
                r.max_body_size(10 * 1024 * 1024)
                    .timeout(Duration::from_secs(120))
                    .allowed_content_types(["multipart/form-data"])
                    .skip_header("Authorization")
            })),
        )
        .route(
            "/api/health",
            get(health).layer(route_guard(|r| r.skip_all())),
        )
        .layer(guard.layer());

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    println!("Listening on http://localhost:3000");
    axum::serve(listener, app).await.unwrap();
}

async fn create_user() -> &'static str {
    r#"{"created":true}"#
}

async fn upload() -> &'static str {
    r#"{"uploaded":true}"#
}

async fn health() -> &'static str {
    r#"{"status":"ok"}"#
}
```

- [ ] **Step 3: Create axum_migration example**

```rust
// examples/axum_migration.rs

//! Gradual migration using OnViolation::LogAndPass (dry-run mode).
//!
//! Run: `cargo run --example axum_migration`
//!
//! In this mode, violations are logged but requests pass through.
//! Monitor your logs to see what would be rejected, then switch
//! to OnViolation::Reject when ready.
//!
//! Test:
//!   # This will log a warning but still return 200
//!   curl -X POST http://localhost:3000/api/users

use axum::{routing::post, Router};
use tower_request_guard::{OnViolation, RequestGuard};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let guard = RequestGuard::builder()
        .require_header("Authorization")
        .allowed_content_types(["application/json"])
        .on_violation(OnViolation::LogAndPass) // dry-run mode
        .build();

    let app = Router::new()
        .route("/api/users", post(create_user))
        .layer(guard.layer());

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    println!("Listening on http://localhost:3000 (dry-run mode)");
    axum::serve(listener, app).await.unwrap();
}

async fn create_user() -> &'static str {
    r#"{"created":true}"#
}
```

- [ ] **Step 4: Add tracing-subscriber to dev-dependencies for the migration example**

In `Cargo.toml`, add to `[dev-dependencies]`:

```toml
tracing-subscriber = "0.3"
```

- [ ] **Step 5: Verify examples compile**

Run: `cargo build --examples`
Expected: All 3 examples compile

- [ ] **Step 6: Commit**

```bash
git add examples/ Cargo.toml
git commit -m "feat: add axum examples (basic, routes, migration)"
```

---

### Task 15: README

**Files:**
- Create: `README.md`

- [ ] **Step 1: Write README**

Write a comprehensive README following tower-rate-tier's style:
- Title + badge area
- One-line description
- Install snippet
- Quick start example (copy-paste from axum_basic)
- Feature table (from competitive positioning in spec)
- Per-route overrides example
- JSON depth protection example
- OnViolation policies section
- Feature flags table
- Companion crate mention (tower-rate-tier)
- License

- [ ] **Step 2: Verify it renders**

Review the markdown for formatting issues.

- [ ] **Step 3: Commit**

```bash
git add README.md
git commit -m "docs: add README with usage guide and examples"
```

---

### Task 16: Final Verification

- [ ] **Step 1: Run all tests**

Run: `cargo test`
Expected: All tests PASS

- [ ] **Step 2: Run all tests with json feature**

Run: `cargo test --features json`
Expected: All tests PASS (including json_tests)

- [ ] **Step 3: Run clippy**

Run: `cargo clippy -- -D warnings`
Expected: No warnings

- [ ] **Step 4: Run cargo fmt**

Run: `cargo fmt --check`
Expected: No formatting issues

- [ ] **Step 5: Verify examples compile**

Run: `cargo build --examples`
Expected: All examples compile

- [ ] **Step 6: Final commit if any fixes needed**

Only if clippy/fmt required changes:

```bash
git add -A
git commit -m "fix: address clippy warnings and formatting"
```

**Note:** Publishing to crates.io (`cargo publish`) is out of scope for this plan. It requires manual credential setup and final review by the author.

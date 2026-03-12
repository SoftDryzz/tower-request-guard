# tower-request-guard

Request validation middleware for [Tower](https://github.com/tower-rs/tower).

Validates incoming requests before they reach the handler: body size limits, timeouts, content-type enforcement, required headers, and JSON depth protection — all in a single configurable layer.

## Install

```toml
[dependencies]
tower-request-guard = "0.1"

# Optional: JSON depth validation
tower-request-guard = { version = "0.1", features = ["json"] }
```

## Quick Start

```rust
use axum::{routing::{get, post}, Router};
use std::time::Duration;
use tower_request_guard::RequestGuard;

let guard = RequestGuard::builder()
    .max_body_size(1_048_576)                      // 1 MB
    .timeout(Duration::from_secs(30))              // 30s
    .allowed_content_types(["application/json"])    // JSON only
    .require_header("Authorization")
    .build();

let app = Router::new()
    .route("/api/users", get(list_users).post(create_user))
    .layer(guard.layer());
```

## Per-Route Overrides

Use `route_guard` to override global settings for specific routes:

```rust
use tower_request_guard::{route_guard, RequestGuard};

let app = Router::new()
    .route("/api/users", post(create_user))
    .route(
        "/api/upload",
        post(upload).layer(route_guard(|r| {
            r.max_body_size(10 * 1024 * 1024)          // 10 MB for uploads
                .timeout(Duration::from_secs(120))
                .allowed_content_types(["multipart/form-data"])
                .skip_header("Authorization")           // uploads don't need auth
        })),
    )
    .route(
        "/api/health",
        get(health).layer(route_guard(|r| r.skip_all())),  // no validations
    )
    .layer(guard.layer());
```

## OnViolation Policies

Control what happens when a violation is detected:

```rust
use tower_request_guard::{OnViolation, ViolationAction};

// Reject (default) — returns appropriate 4xx/5xx immediately
.on_violation(OnViolation::Reject)

// Log and pass — dry-run mode for gradual migration
.on_violation(OnViolation::LogAndPass)

// Custom — full control with callback
.on_violation(OnViolation::custom(|violation| {
    tracing::warn!(?violation, "request guard violation");
    ViolationAction::Reject
}))
```

## JSON Depth Protection

Enable the `json` feature for anti-JSON-bomb protection:

```rust
use tower_request_guard::{BufferedRequestGuardLayer, RequestGuard};

let guard = RequestGuard::builder()
    .max_json_depth(32)                            // max nesting depth
    .max_body_size(1_048_576)                      // also checked post-buffering
    .build();

let layer = BufferedRequestGuardLayer::new(guard);
```

The buffered variant reads the full body, checks size, validates JSON depth, then forwards the request with a `Full<Bytes>` body.

## Violation Responses

Each violation returns a JSON body with context:

| Violation | Status | Error Key |
|-----------|--------|-----------|
| Body exceeds max size | 413 Payload Too Large | `body_too_large` |
| Timeout expired | 504 Gateway Timeout | `request_timeout` |
| Content-Type not allowed | 415 Unsupported Media Type | `invalid_content_type` |
| Required header missing | 400 Bad Request | `missing_header` |
| JSON depth exceeded | 400 Bad Request | `json_too_deep` |
| Malformed JSON | 400 Bad Request | `invalid_json` |

Example response:

```json
{"error":"payload too large","violation":"body_too_large","max":1048576,"received":5242880}
```

## Feature Flags

| Feature | Default | Description |
|---------|---------|-------------|
| `json` | No | JSON depth validation via `serde_json` + body buffering via `http-body-util` |

## Comparison

| Feature | tower-http (multiple layers) | **tower-request-guard** |
|---------|------------------------------|------------------------|
| Max body size | `RequestBodyLimitLayer` | Yes |
| Per-route timeout | `TimeoutLayer` (global only) | Yes |
| Content-Type validation | No | Yes (media type matching) |
| Required headers (N) | `ValidateRequestHeader` (1) | Yes |
| JSON depth (anti bomb) | No | Yes (feature "json") |
| All in one layer | No (3-4 separate layers) | Yes |
| Per-route overrides | No | Yes (`route_guard`) |
| Dry-run mode | No | Yes (`LogAndPass`) |
| Bodyless method skip | Manual | Automatic |
| Custom violation handler | No | Yes (`OnViolation::Custom`) |

## Companion Crate

**[tower-rate-tier](https://github.com/SoftDryzz/tower-rate-tier)** — Rate limiting middleware for Tower.

Together: **rate-tier** controls *how many times* you can call, **request-guard** validates *what you send* is correct and safe.

## License

MIT OR Apache-2.0

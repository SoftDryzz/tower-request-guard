# tower-request-guard

**Request validation middleware for Tower.**

[![Crates.io](https://img.shields.io/crates/v/tower-request-guard.svg?v=1)](https://crates.io/crates/tower-request-guard)
[![Documentation](https://docs.rs/tower-request-guard/badge.svg?v=1)](https://docs.rs/tower-request-guard)
[![License](https://img.shields.io/crates/l/tower-request-guard.svg?v=1)](LICENSE-MIT)

Every API needs input validation before the handler runs. `tower-request-guard` replaces 3-4 separate Tower layers with a single configurable middleware: body size limits, timeouts, content-type enforcement, required headers, and JSON depth protection.

## Features

- **Max body size** — Reject oversized payloads via Content-Length pre-check
- **Per-route timeout** — 504 Gateway Timeout with configurable duration per route
- **Content-Type validation** — Media type matching with charset/parameter tolerance
- **Required headers** — Enforce N headers (Authorization, X-Request-Id, etc.)
- **JSON depth protection** — Anti-JSON-bomb via max nesting depth (feature `json`)
- **Per-route overrides** — Override any setting per route with `route_guard`
- **Dry-run mode** — `LogAndPass` logs violations without rejecting (gradual migration)
- **Custom violation handler** — Full control with `OnViolation::custom()`
- **Bodyless method skip** — GET/HEAD/DELETE/OPTIONS skip body checks automatically
- **Tower-native** — Works with Axum, Tonic, Hyper, or any Tower-based framework

## Quick Start

Add to your `Cargo.toml`:

```toml
[dependencies]
tower-request-guard = "0.1"
```

### Configure the Guard

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

### Per-Route Overrides

Use `route_guard` to override global settings for specific routes. Apply it as the **outer layer** relative to the guard — it inserts config into request extensions before the guard reads them.

In Axum, use separate sub-routers and merge them:

```rust
use tower_request_guard::{route_guard, RequestGuard};

let guard_layer = guard.layer(); // Arc inside, cheap to clone

// Standard routes — global guard applies as-is
let api = Router::new()
    .route("/api/users", post(create_user))
    .layer(guard_layer.clone());

// Upload — larger limits, different content type, no auth
let upload = Router::new()
    .route("/api/upload", post(upload))
    .layer(guard_layer.clone())              // inner: validates
    .layer(route_guard(|r| {                 // outer: inserts overrides
        r.max_body_size(10 * 1024 * 1024)
            .timeout(Duration::from_secs(120))
            .allowed_content_types(["multipart/form-data"])
            .skip_header("Authorization")
    }));

// Health — skip all validations
let health = Router::new()
    .route("/api/health", get(health))
    .layer(guard_layer)
    .layer(route_guard(|r| r.skip_all()));

let app = api.merge(upload).merge(health);
```

### OnViolation Policies

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

### JSON Depth Protection

Enable the `json` feature for anti-JSON-bomb protection:

```toml
tower-request-guard = { version = "0.1", features = ["json"] }
```

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
| JSON depth (anti bomb) | No | Yes (feature `json`) |
| All in one layer | No (3-4 separate layers) | Yes |
| Per-route overrides | No | Yes (`route_guard`) |
| Dry-run mode | No | Yes (`LogAndPass`) |
| Bodyless method skip | Manual | Automatic |
| Custom violation handler | No | Yes (`OnViolation::Custom`) |

## Examples

See the [`examples/`](examples/) directory:

- [`axum_basic.rs`](examples/axum_basic.rs) — Simple global guard with Axum
- [`axum_routes.rs`](examples/axum_routes.rs) — Per-route overrides using sub-router + merge pattern
- [`axum_migration.rs`](examples/axum_migration.rs) — LogAndPass dry-run mode with tracing

## Companion Crate

**[tower-rate-tier](https://github.com/SoftDryzz/tower-rate-tier)** — Tier-based rate limiting middleware for Tower.

Together: **rate-tier** controls *how many times* you can call, **request-guard** validates *what you send* is correct and safe.

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT License ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.

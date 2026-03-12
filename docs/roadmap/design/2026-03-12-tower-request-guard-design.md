# tower-request-guard — Design Spec

**Date:** 2026-03-12
**Author:** SoftDryzz (OpenCode)
**Status:** Draft
**Tagline:** Request validation middleware for Tower

---

## 1. Problem Statement

Every SaaS API needs to validate incoming requests before processing them: limit body size, enforce timeouts, require specific content types and headers, protect against JSON bombs. In Rust, the current options are fragmented:

- **tower-http `RequestBodyLimitLayer`** — Body size only, nothing else.
- **tower-http `TimeoutLayer`** — Global timeout only, no per-route configuration.
- **tower-http `ValidateRequestHeaderLayer`** — Validates a single header value, not multiple required headers.
- **Manual middleware** — Every team writes 100-300 lines of custom middleware combining 3-4 separate layers.

**Result:** There is no single, configurable layer that combines all request validations. Developers either stack multiple uncoordinated layers or write custom middleware from scratch.

## 2. Solution

`tower-request-guard` is a Tower middleware library that validates requests before they reach the handler, with:

- Max body size (Content-Length pre-check + stream limit)
- Request timeout per route
- Allowed content types (media type matching)
- Required headers
- Max JSON depth — anti JSON bomb (feature-gated)
- Configurable violation policy (reject / log-and-pass / custom)
- Per-route overrides via `route_guard`

It is the natural complement to `tower-rate-tier`: **rate-tier controls how many times you can call, request-guard validates that what you send is correct and safe.**

## 3. Target Users

- Rust developers building SaaS APIs with Axum, Tonic, Hyper, or any Tower-based framework
- Teams that currently write manual request validation in each handler
- Users of `tower-rate-tier` who want the complementary request protection layer

## 4. API Design

### 4.1 Global Builder

```rust
use tower_request_guard::{RequestGuard, OnViolation};
use std::time::Duration;

let guard = RequestGuard::builder()
    .max_body_size(1_048_576)                          // 1 MB
    .timeout(Duration::from_secs(30))                  // 30s
    .allowed_content_types(["application/json"])        // JSON only
    .require_header("Authorization")
    .require_header("X-Request-Id")
    .max_json_depth(32)                                // feature = "json"
    .on_violation(OnViolation::Reject)                 // default
    .build();
```

### 4.2 Applying the Middleware

```rust
use axum::{Router, routing::{get, post}};
use tower_request_guard::route_guard;

let app = Router::new()
    .route("/api/users", get(list_users))              // uses global defaults
    .route("/api/upload", post(upload).layer(route_guard(|r| {
        r.max_body_size(10 * 1024 * 1024)              // 10 MB for uploads
         .timeout(Duration::from_secs(120))
         .allowed_content_types(["multipart/form-data"])
         .skip_header("Authorization")                  // uploads don't need auth
    })))
    .route("/api/health", get(health).layer(route_guard(|r| {
        r.skip_all()                                    // no validations
    })))
    .layer(guard.layer());
```

### 4.3 OnViolation Policies

```rust
// Reject (default) — returns appropriate 4xx immediately
.on_violation(OnViolation::Reject)

// Log and pass — logs the violation but forwards request (dry-run / migration)
.on_violation(OnViolation::LogAndPass)

// Custom — callback for metrics/tracing/custom responses
// Callback must be Fn(&Violation) -> ViolationAction + Send + Sync + 'static
.on_violation(OnViolation::custom(|violation: &Violation| -> ViolationAction {
    tracing::warn!(?violation, "request guard violation");
    ViolationAction::Reject                           // use default response
    // ViolationAction::Pass                          // let it through (ignored for Timeout)
    // ViolationAction::RespondWith(my_response)      // fully custom response
}))
```

### 4.4 Violation Types and HTTP Status Codes

| Violation | Status Code | Error key |
|-----------|-------------|-----------|
| Body exceeds max size | 413 Payload Too Large | `body_too_large` |
| Timeout expired | 504 Gateway Timeout | `request_timeout` |
| Content-Type not allowed | 415 Unsupported Media Type | `invalid_content_type` |
| Required header missing | 400 Bad Request | `missing_header` |
| JSON depth exceeded | 400 Bad Request | `json_too_deep` |
| Malformed JSON (feature "json") | 400 Bad Request | `invalid_json` |

**Note on timeout status code:** `504 Gateway Timeout` is used instead of `408 Request Timeout` because RFC 9408 defines 408 as "the client was too slow sending the request." Here the server-side handler exceeded its processing budget, which is a gateway/proxy timeout semantic.

**Note on timeout and OnViolation:** Timeout violations always result in a response (504 or custom via `OnViolation::Custom`). `OnViolation::LogAndPass` does NOT apply to timeouts — once the inner service is cancelled, there is no response to forward. The custom callback receives a `Violation::Timeout` and can return `ViolationAction::RespondWith(response)` for a custom 504 body, but `ViolationAction::Pass` is ignored for this violation type.

Each violation returns a JSON body with context specific to the violation type:

```json
{"error": "payload too large", "violation": "body_too_large", "max": 1048576, "received": 5242880}
```

```json
{"error": "missing required header", "violation": "missing_header", "header": "Authorization"}
```

```json
{"error": "unsupported content type", "violation": "invalid_content_type", "received": "text/xml", "allowed": ["application/json"]}
```

```json
{"error": "json depth exceeded", "violation": "json_too_deep", "max_depth": 32, "found_depth": 128}
```

```json
{"error": "request timeout", "violation": "request_timeout", "timeout_ms": 30000}
```

```json
{"error": "invalid json", "violation": "invalid_json", "detail": "unexpected EOF at line 1 column 42"}
```

**Note:** JSON error responses are built with `format!()`, not serde, to avoid a mandatory serde dependency. All interpolated values (header names, content types, error messages) are sanitized with a minimal JSON string escaper to prevent malformed responses from untrusted input.

### 4.5 route_guard Merging

`route_guard` inserts a `RouteGuardConfig` into request extensions. The `RequestGuardService` reads it and merges with global defaults:

- Numeric values (`max_body_size`, `timeout`, `max_json_depth`): route override replaces global
- Lists (`allowed_content_types`): route override replaces entirely
- Required headers: inherited from global, `skip_header("X")` removes individual ones, `require_header("X")` adds route-specific ones
- `skip_all()`: disables all validations for that route

### 4.6 Bodyless Method Handling

Requests with methods that typically carry no body (GET, HEAD, DELETE, OPTIONS) automatically skip body-related validations:

- `max_body_size` — skipped
- `allowed_content_types` — skipped
- `max_json_depth` — skipped

Header validations and timeout still apply. This avoids false positives without requiring per-route configuration.

### 4.7 Content-Type Matching

Content-Type comparison matches the media type only, ignoring parameters. `"application/json"` in the config matches `"application/json; charset=utf-8"` from the client. This prevents rejecting valid requests from clients that include charset or boundary parameters.

### 4.8 Body Size Enforcement

Body size is enforced with a layered strategy:

1. **Content-Length pre-check (always)** — If the `Content-Length` header is present and exceeds `max_body_size`, the request is rejected immediately without reading a single byte. This covers the majority of cases at O(1) cost.
2. **Post-buffering check (feature "json")** — When the `json` feature is enabled, the body is fully buffered before JSON depth validation. The buffered size is checked against `max_body_size`, catching chunked transfers that lack `Content-Length`.

**Note on chunked transfers without `json` feature:** The non-buffered service path cannot enforce stream-level body limits without changing the body's generic type (same constraint as `tower-http::RequestBodyLimitLayer`, which also transforms the body type). For applications that need stream-level enforcement of chunked bodies without the `json` feature, combine with `tower-http::RequestBodyLimitLayer` on the outer layer.

## 5. Architecture

### 5.1 Crate Structure

```
tower-request-guard/
├── src/
│   ├── lib.rs              # Public re-exports
│   ├── guard.rs            # RequestGuard, RequestGuardBuilder
│   ├── layer.rs            # RequestGuardLayer (implements tower::Layer)
│   ├── service.rs          # RequestGuardService (implements tower::Service)
│   ├── route.rs            # route_guard() helper, RouteGuardConfig
│   ├── violation.rs        # Violation enum, ViolationAction, OnViolation
│   ├── response.rs         # Error response builder (JSON bodies per violation)
│   ├── body.rs             # Body size enforcement (Content-Length + stream limit)
│   ├── content_type.rs     # Content-Type matching (media type only, ignores params)
│   ├── headers.rs          # Required headers validation
│   ├── timeout.rs          # Timeout wrapping (tokio::time::timeout)
│   └── json.rs             # JSON depth validation (feature = "json")
├── tests/
│   ├── integration.rs      # Full middleware integration tests
│   ├── body_tests.rs       # Body size validation tests
│   ├── content_type_tests.rs
│   ├── headers_tests.rs
│   ├── timeout_tests.rs
│   └── json_tests.rs       # JSON depth tests (feature = "json")
├── examples/
│   ├── axum_basic.rs       # Basic Axum setup
│   ├── axum_routes.rs      # Per-route overrides with route_guard
│   └── axum_migration.rs   # OnViolation::LogAndPass for gradual migration
├── Cargo.toml
├── README.md
└── LICENSE                 # MIT OR Apache-2.0
```

### 5.2 Core Flow

```
Request arrives
    │
    ▼
RequestGuardService receives request
    │
    ▼
Read RouteGuardConfig from extensions (if any)
Merge with global config
    │
    ▼
Is bodyless method (GET/HEAD/DELETE/OPTIONS)?
    ├─ Yes → skip body/content-type/json checks
    │
    ▼
1. Content-Type check (header only, O(1))
    ├─ Violation → OnViolation policy → 415 or pass
    │
    ▼
2. Required headers check (header only, O(n))
    ├─ Violation → OnViolation policy → 400 or pass
    │
    ▼
3. Content-Length pre-check (header only, O(1))
    ├─ Exceeds limit → OnViolation policy → 413 or pass
    │
    ▼
4. JSON depth check [feature = "json", only if max_json_depth configured for this route]
    ├─ Buffer body (up to max_body_size)
    ├─ Parse JSON: if malformed → OnViolation policy → 400 (invalid_json) or pass
    ├─ Validate depth: if exceeded → OnViolation policy → 400 (json_too_deep) or pass
    ├─ Reconstruct body as Full<Bytes>
    │
    ▼
5. Body stream limit (if no Content-Length AND body was not already buffered by step 4)
    ├─ Wrap body with stream limiter
    │
    ▼
6. Timeout wrap (tokio::time::timeout around inner service call)
    ├─ Expired → 504 (OnViolation::LogAndPass ignored for timeouts)
    │
    ▼
Inner Service → Response
```

**Validation order:** Cheapest checks first (headers only), then body checks, then timeout wrapping. This ensures fail-fast behavior — most invalid requests are caught before reading the body.

### 5.3 Two Service Variants

```rust
// Without feature "json" — body type passes through transparently
RequestGuardService<S, B>  implements Service<Request<B>>

// With feature "json" — buffers body, output is Full<Bytes>
BufferedRequestGuardService<S>  implements Service<Request<Body>>
```

When the `json` feature is enabled and `max_json_depth` is called on the builder, `guard.layer()` returns a `BufferedRequestGuardLayer` instead of `RequestGuardLayer`. This mirrors tower-rate-tier's approach of having a buffered service variant when body access is needed.

**Why implicit:** If you configure `max_json_depth`, you obviously need body buffering. Requiring an explicit `.buffer_body()` call would be redundant ceremony. The feature flag `json` already signals intent. This behavior is documented on the builder's `max_json_depth()` method.

### 5.4 Error Handling

The Tower `Service::Error` type is the inner service's error type. Request guard violations are **never** propagated as `Service::Error`:

- **4xx violations** → normal HTTP responses, not errors
- **OnViolation::LogAndPass** → logs via `tracing::warn!`, forwards to inner service
- **OnViolation::Custom** → delegates to user callback

This ensures the middleware never breaks the Tower service contract.

### 5.5 Dependencies

```toml
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

**Note:** `serde` and `serde_json` are NOT required dependencies. Error JSON responses are built with `format!()`. The `serde_json` dependency only enters via the `json` feature for JSON depth validation.

## 6. Competitive Positioning

| Feature | tower-http (multiple layers) | Manual middleware | **tower-request-guard** |
|---------|------------------------------|-------------------|------------------------|
| Max body size | `RequestBodyLimitLayer` | Custom | **Yes** |
| Per-route timeout | `TimeoutLayer` (global only) | Custom | **Yes** |
| Content-Type validation | No | Custom | **Yes (media type matching)** |
| Required headers (N) | `ValidateRequestHeader` (1) | Custom | **Yes** |
| JSON depth (anti bomb) | No | Custom | **Yes (feature "json")** |
| All in one layer | No (3-4 separate layers) | No | **Yes** |
| Per-route overrides | No | Custom | **Yes (`route_guard`)** |
| Dry-run mode | No | Custom | **Yes (`LogAndPass`)** |
| Bodyless method skip | Manual | Manual | **Automatic** |
| Custom violation handler | No | Custom | **Yes (`OnViolation::Custom`)** |
| Content-Length pre-check | Yes (v0.5+) | Custom | **Yes (rejects before reading)** |
| Tower-compatible | Yes | Depends | **Yes** |

## 7. Success Criteria

### v0.1.0 (MVP)
- [ ] Core: RequestGuard builder with all validations
- [ ] Tower Layer/Service implementation
- [ ] Content-Type matching (media type only, ignores params)
- [ ] Required headers validation
- [ ] Body size: Content-Length pre-check + stream limit
- [ ] Timeout wrapping with tokio::time::timeout
- [ ] OnViolation: Reject / LogAndPass / Custom with RespondWith
- [ ] route_guard per-route overrides with config merging
- [ ] Automatic skip for bodyless methods (GET/HEAD/DELETE/OPTIONS)
- [ ] JSON error responses with per-violation context
- [ ] JSON depth validation (feature "json")
- [ ] Examples: axum_basic, axum_routes, axum_migration
- [ ] README with usage guide
- [ ] Published to crates.io

### v0.2.0
- [ ] Metrics/events hook (on_violation callback with metrics)
- [ ] Custom response format trait (Problem Details RFC 9457)
- [ ] Tonic/gRPC example

### v0.3.0
- [ ] Request schema validation (feature "schema", JSON Schema)
- [ ] Violation rate tracking per IP (integration with tower-rate-tier)
- [ ] Dashboard-ready metrics export

## 8. License

Dual-licensed under MIT OR Apache-2.0 (standard for Rust ecosystem libraries, maximizes adoption).

## 9. Marketing Strategy

1. **Cross-promotion** — Reference in tower-rate-tier README as the natural complement
2. **r/rust post** — "tower-request-guard: protect your API requests before they hit the handler" with comparison table
3. **Comparison table** — Show how it replaces 3-4 tower-http layers with one configurable layer
4. **README-driven** — Excellent README with copy-paste examples that compile and run
5. **Examples that work** — Runnable examples demonstrating real use cases

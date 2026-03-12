# Roadmap

## v0.1 — Core Guard (current)

- [x] Max body size validation (Content-Length pre-check)
- [x] Per-route request timeout (504 Gateway Timeout)
- [x] Content-Type enforcement (media type matching, charset tolerance)
- [x] Required headers validation (N headers, case-insensitive)
- [x] JSON depth protection via `json` feature (anti-JSON-bomb)
- [x] Buffered body variant for chunked transfer + JSON depth
- [x] Per-route overrides with `route_guard`
- [x] OnViolation policies: Reject, LogAndPass, Custom
- [x] Bodyless method auto-skip (GET/HEAD/DELETE/OPTIONS)
- [x] JSON error responses with violation context

## v0.2 — Enhanced Validation

- [ ] Streaming JSON depth checker (avoid full deserialization, O(1) memory)
- [ ] Size-limited body collection (abort buffering once `max_body_size` exceeded)
- [ ] Request body stream limiting (chunked transfers without full buffering)
- [ ] IP-based allowlist/denylist
- [ ] Custom validation hooks (user-defined `Fn(&Request) -> Result<(), Violation>`)
- [ ] Metrics integration (counters per violation type via `metrics` crate)
- [ ] `tracing::Span` enrichment with guard decision metadata

## v0.3 — Advanced Features

- [ ] Rate-aware coordination with [tower-rate-tier](https://github.com/SoftDryzz/tower-rate-tier)
- [ ] Request signature validation (HMAC, API key format)
- [ ] Schema validation for JSON payloads (feature-gated)
- [ ] Configurable error response format (JSON, plain text, custom)

## Future

- [ ] OpenTelemetry span attributes for guard decisions
- [ ] `no_std` compatible core (validation logic without HTTP types)
- [ ] WASM target support

---

Have an idea? Open an [issue](https://github.com/SoftDryzz/tower-request-guard/issues) with the `enhancement` label.

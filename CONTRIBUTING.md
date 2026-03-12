# Contributing to tower-request-guard

Thanks for your interest in contributing! This guide explains how to report bugs, suggest features, and submit code.

## Reporting Bugs

Open an [issue](https://github.com/SoftDryzz/tower-request-guard/issues) with:

- **Title**: Short description of the problem
- **Rust version**: Output of `rustc --version`
- **Crate version**: The version of `tower-request-guard` you're using
- **Minimal reproduction**: The smallest code that triggers the bug
- **Expected vs actual behavior**: What you expected and what happened
- **Logs/errors**: Any relevant error messages or panics

## Suggesting Features

Open an issue with the label `enhancement` and include:

- **Use case**: What problem does this solve?
- **Proposed API**: How would the feature look from the user's perspective?
- **Alternatives considered**: Other approaches you've thought about

## Submitting Code

### Setup

```bash
git clone https://github.com/SoftDryzz/tower-request-guard.git
cd tower-request-guard
cargo test
cargo test --features json
```

### Branch Naming

Create your branch from `main` using this convention:

| Type | Pattern | Example |
|------|---------|---------|
| Feature | `feat/<username>/<short-description>` | `feat/alice/ip-allowlist` |
| Bug fix | `fix/<username>/<short-description>` | `fix/bob/charset-matching` |
| Docs | `docs/<username>/<short-description>` | `docs/carol/tonic-example` |

### Commit Messages

We use [Conventional Commits](https://www.conventionalcommits.org/) in English:

```
feat: add IP-based allowlist validation
fix: handle missing Content-Type on multipart uploads
docs: add Tonic/gRPC usage example
test: add integration tests for LogAndPass mode
refactor: simplify content-type matching logic
```

### Pull Requests

1. Fork the repo and create your branch from `main`
2. Write or update tests for your changes
3. Run all checks before submitting:
   ```bash
   cargo fmt --check
   cargo clippy -- -D warnings
   cargo test
   cargo test --features json
   ```
4. Open a PR against `main` with a clear description of what and why
5. Link any related issues

### Code Style

- Run `cargo fmt` before committing
- No Clippy warnings (`cargo clippy -- -D warnings`)
- Add doc comments (`///`) for all public items
- Keep unsafe code out — this crate is 100% safe Rust
- Prefer simple, readable code over clever abstractions

### Tests

- Unit tests go in the same file as the code (`#[cfg(test)]` module)
- Integration tests go in `tests/`
- Feature-gated code needs tests behind the same feature flag
- Use `tokio::test` for async tests

## Architecture Overview

If you want to understand the codebase before contributing:

- `src/guard.rs` — `RequestGuard` and builder, the main entry point
- `src/service.rs` — Tower Service that enforces request validation
- `src/layer.rs` — Tower Layer implementation
- `src/violation.rs` — Violation types, actions, and `OnViolation` policies
- `src/route.rs` — Per-route overrides via `route_guard`
- `src/response.rs` — JSON error response generation
- `src/body.rs` — Body size checks and bodyless-method detection
- `src/content_type.rs` — Content-Type media type matching
- `src/headers.rs` — Required header validation
- `src/json.rs` — JSON depth validation (feature: `json`)
- `src/buffered.rs` — Buffered body variant for JSON depth (feature: `json`)

## Questions?

Open a [discussion](https://github.com/SoftDryzz/tower-request-guard/discussions) or an issue. No question is too small.

## License

By contributing, you agree that your contributions will be licensed under MIT OR Apache-2.0, the same as the project.

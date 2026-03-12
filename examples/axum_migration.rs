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

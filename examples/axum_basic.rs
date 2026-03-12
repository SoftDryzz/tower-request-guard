//! Basic request guard setup with Axum.
//!
//! Run: `cargo run --example axum_basic`
//!
//! Test:
//!   curl -X POST -H "Content-Type: application/json" \
//!        -H "Authorization: Bearer token" \
//!        -d '{"hello":"world"}' \
//!        http://localhost:3000/api/users

use axum::{routing::get, Router};
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

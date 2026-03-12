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

use axum::{
    routing::{get, post},
    Router,
};
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

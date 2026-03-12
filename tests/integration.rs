use http::{Request, Response, StatusCode};
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;
use tower_layer::Layer;
use tower_service::Service;

use tower_request_guard::{OnViolation, RequestGuard, Violation, ViolationAction, route_guard};

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

// ── Route Guard Tests ────────────────────────────────────────────────

#[tokio::test]
async fn route_guard_overrides_body_size() {
    let guard = RequestGuard::builder()
        .max_body_size(100)
        .build();

    let route_layer = route_guard(|r| r.max_body_size(10_000));

    // route_guard must be outer so it inserts config before the guard reads it
    let mut svc = route_layer.layer(guard.layer().layer(OkService));
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
    let mut svc = route_layer.layer(guard.layer().layer(OkService));

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
    let mut svc = route_layer.layer(guard.layer().layer(OkService));

    let req = Request::builder()
        .method("POST")
        .uri("/test")
        .header("x-request-id", "abc")
        .body(String::new())
        .unwrap();

    let resp = svc.call(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

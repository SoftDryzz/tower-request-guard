use bytes::Bytes;
use http::{Request, Response, StatusCode};
use http_body_util::Full;
use std::task::{Context, Poll};
use tower_layer::Layer;
use tower_service::Service;

use tower_request_guard::{BufferedRequestGuardLayer, RequestGuard};

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

    let req = Request::builder()
        .method("POST")
        .uri("/test")
        .header("content-type", "application/json")
        .body(Full::new(Bytes::from(r#"{"large":"this body is definitely more than 10 bytes"}"#)))
        .unwrap();

    let resp = svc.call(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::PAYLOAD_TOO_LARGE);
}

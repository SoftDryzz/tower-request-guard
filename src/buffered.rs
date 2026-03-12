use crate::body::{check_content_length, is_bodyless_method};
use crate::content_type::matches_content_type;
use crate::guard::RequestGuard;
use crate::headers::find_missing_header;
use crate::json::{check_json_depth, JsonDepthError};
use crate::response::violation_response;
use crate::route::RouteGuardConfig;
use crate::service::{handle_timeout_violation, handle_violation};
use crate::violation::Violation;
use bytes::Bytes;
use http::{Request, Response};
use http_body_util::BodyExt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tower_layer::Layer;
use tower_service::Service;

/// Tower Layer for the buffered variant (json feature enabled).
#[derive(Clone)]
pub struct BufferedRequestGuardLayer {
    pub(crate) guard: Arc<RequestGuard>,
}

impl BufferedRequestGuardLayer {
    pub fn new(guard: RequestGuard) -> Self {
        Self {
            guard: Arc::new(guard),
        }
    }
}

impl<S> Layer<S> for BufferedRequestGuardLayer {
    type Service = BufferedRequestGuardService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        BufferedRequestGuardService {
            inner,
            guard: self.guard.clone(),
        }
    }
}

/// Tower Service that buffers the body for JSON depth validation.
pub struct BufferedRequestGuardService<S> {
    pub(crate) inner: S,
    pub(crate) guard: Arc<RequestGuard>,
}

impl<S: Clone> Clone for BufferedRequestGuardService<S> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            guard: self.guard.clone(),
        }
    }
}

impl<S, B, ResBody> Service<Request<B>> for BufferedRequestGuardService<S>
where
    S: Service<Request<http_body_util::Full<Bytes>>, Response = Response<ResBody>>
        + Clone
        + Send
        + 'static,
    S::Future: Send,
    S::Error: Send,
    B: http_body::Body<Data = Bytes> + Send + 'static,
    B::Error: std::fmt::Display,
    ResBody: From<String> + Send,
{
    type Response = Response<ResBody>;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<B>) -> Self::Future {
        let guard = self.guard.clone();
        let mut inner = self.inner.clone();
        std::mem::swap(&mut self.inner, &mut inner);

        Box::pin(async move {
            let effective = match req.extensions().get::<RouteGuardConfig>() {
                Some(route_config) => route_config.merge_with(&guard.config),
                None => guard.config.clone(),
            };

            let is_bodyless = is_bodyless_method(req.method());

            // 1. Content-Type check
            if !is_bodyless {
                if let Some(ref allowed) = effective.allowed_content_types {
                    let content_type = req
                        .headers()
                        .get("content-type")
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("");
                    if !matches_content_type(content_type, allowed) {
                        let violation = Violation::InvalidContentType {
                            received: content_type.to_string(),
                            allowed: allowed.clone(),
                        };
                        if let Some(resp) = handle_violation(&violation, &guard.on_violation) {
                            return Ok(resp.map(Into::into));
                        }
                    }
                }
            }

            // 2. Required headers check
            if !effective.required_headers.is_empty() {
                if let Some(missing) =
                    find_missing_header(req.headers(), &effective.required_headers)
                {
                    let violation = Violation::MissingHeader { header: missing };
                    if let Some(resp) = handle_violation(&violation, &guard.on_violation) {
                        return Ok(resp.map(Into::into));
                    }
                }
            }

            // 3. Content-Length pre-check
            if !is_bodyless {
                if let Some(max) = effective.max_body_size {
                    if let Some(received) = check_content_length(req.headers(), max) {
                        let violation = Violation::BodyTooLarge { max, received };
                        if let Some(resp) = handle_violation(&violation, &guard.on_violation) {
                            return Ok(resp.map(Into::into));
                        }
                    }
                }
            }

            // 4. Buffer body and check JSON depth
            let (parts, body) = req.into_parts();
            let body_bytes = match body.collect().await {
                Ok(collected) => collected.to_bytes(),
                Err(e) => {
                    let violation = Violation::InvalidJson {
                        detail: e.to_string(),
                    };
                    if let Some(resp) = handle_violation(&violation, &guard.on_violation) {
                        return Ok(resp.map(Into::into));
                    }
                    Bytes::new()
                }
            };

            // Check body size (for chunked transfers without Content-Length)
            if !is_bodyless {
                if let Some(max) = effective.max_body_size {
                    if body_bytes.len() as u64 > max {
                        let violation = Violation::BodyTooLarge {
                            max,
                            received: body_bytes.len() as u64,
                        };
                        if let Some(resp) = handle_violation(&violation, &guard.on_violation) {
                            return Ok(resp.map(Into::into));
                        }
                    }
                }
            }

            // JSON depth check (only for JSON content-type and non-empty body)
            if !is_bodyless && !body_bytes.is_empty() {
                if let Some(max_depth) = effective.max_json_depth {
                    let is_json = parts
                        .headers
                        .get("content-type")
                        .and_then(|v| v.to_str().ok())
                        .map(|ct| ct.to_ascii_lowercase().contains("application/json"))
                        .unwrap_or(false);

                    if is_json {
                        match check_json_depth(&body_bytes, max_depth) {
                            Ok(_) => {}
                            Err(JsonDepthError::TooDeep {
                                max_depth,
                                found_depth,
                            }) => {
                                let violation = Violation::JsonTooDeep {
                                    max_depth,
                                    found_depth,
                                };
                                if let Some(resp) =
                                    handle_violation(&violation, &guard.on_violation)
                                {
                                    return Ok(resp.map(Into::into));
                                }
                            }
                            Err(JsonDepthError::Malformed { detail }) => {
                                let violation = Violation::InvalidJson { detail };
                                if let Some(resp) =
                                    handle_violation(&violation, &guard.on_violation)
                                {
                                    return Ok(resp.map(Into::into));
                                }
                            }
                        }
                    }
                }
            }

            // Reconstruct request with Full<Bytes> body
            let new_req = Request::from_parts(parts, http_body_util::Full::new(body_bytes));

            // 6. Timeout wrap
            if let Some(timeout_duration) = effective.timeout {
                match tokio::time::timeout(timeout_duration, inner.call(new_req)).await {
                    Ok(result) => result,
                    Err(_elapsed) => {
                        let violation = Violation::RequestTimeout {
                            timeout_ms: timeout_duration.as_millis() as u64,
                        };
                        Ok(handle_timeout_violation(&violation, &guard.on_violation)
                            .map(Into::into))
                    }
                }
            } else {
                inner.call(new_req).await
            }
        })
    }
}

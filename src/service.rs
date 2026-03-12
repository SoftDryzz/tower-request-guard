use crate::body::{check_content_length, is_bodyless_method};
use crate::content_type::matches_content_type;
use crate::guard::RequestGuard;
use crate::headers::find_missing_header;
use crate::response::violation_response;
use crate::route::RouteGuardConfig;
use crate::violation::{OnViolation, Violation, ViolationAction};
use http::{Request, Response};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tower_service::Service;

/// Tower Service that validates requests before forwarding.
pub struct RequestGuardService<S> {
    pub(crate) inner: S,
    pub(crate) guard: Arc<RequestGuard>,
}

impl<S: Clone> Clone for RequestGuardService<S> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            guard: self.guard.clone(),
        }
    }
}

impl<S, B, ResBody> Service<Request<B>> for RequestGuardService<S>
where
    S: Service<Request<B>, Response = Response<ResBody>> + Clone + Send + 'static,
    S::Future: Send,
    S::Error: Send,
    B: Send + 'static,
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
            // Resolve effective config (merge route overrides)
            let effective = match req.extensions().get::<RouteGuardConfig>() {
                Some(route_config) => route_config.merge_with(&guard.config),
                None => guard.config.clone(),
            };

            let is_bodyless = is_bodyless_method(req.method());

            // 1. Content-Type check (skip for bodyless methods)
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

            // 3. Content-Length pre-check (skip for bodyless methods)
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

            // Steps 4-5 (JSON depth + full body buffering with size check)
            // are handled by BufferedRequestGuardService when json feature is enabled.
            //
            // Body size enforcement strategy for non-buffered path:
            // - Content-Length present: rejected in step 3 above (O(1), no body read)
            // - Content-Length absent (chunked): not enforced here (would require
            //   changing the body type to LimitedBody<B>, breaking the generic service
            //   contract). For full stream limiting of chunked bodies, enable the
            //   `json` feature (which buffers and checks) or combine with
            //   tower-http::RequestBodyLimitLayer.

            // 6. Timeout wrap
            if let Some(timeout_duration) = effective.timeout {
                match tokio::time::timeout(timeout_duration, inner.call(req)).await {
                    Ok(result) => result,
                    Err(_elapsed) => {
                        let violation = Violation::RequestTimeout {
                            timeout_ms: u64::try_from(timeout_duration.as_millis())
                                .unwrap_or(u64::MAX),
                        };
                        let resp = handle_timeout_violation(&violation, &guard.on_violation);
                        Ok(resp.map(Into::into))
                    }
                }
            } else {
                inner.call(req).await
            }
        })
    }
}

/// Handle a pre-handler violation according to the OnViolation policy.
/// Returns Some(response) if the request should be rejected, None if it should pass.
pub(crate) fn handle_violation(
    violation: &Violation,
    policy: &OnViolation,
) -> Option<Response<String>> {
    match policy {
        OnViolation::Reject => Some(violation_response(violation)),
        OnViolation::LogAndPass => {
            tracing::warn!(?violation, "request guard violation (log-and-pass)");
            None
        }
        OnViolation::Custom(callback) => match callback(violation) {
            ViolationAction::Reject => Some(violation_response(violation)),
            ViolationAction::Pass => None,
            ViolationAction::RespondWith(resp) => Some(resp),
        },
    }
}

/// Handle a timeout violation. LogAndPass is ignored for timeouts.
pub(crate) fn handle_timeout_violation(
    violation: &Violation,
    policy: &OnViolation,
) -> Response<String> {
    match policy {
        OnViolation::Custom(callback) => match callback(violation) {
            ViolationAction::RespondWith(resp) => resp,
            _ => violation_response(violation),
        },
        _ => violation_response(violation),
    }
}

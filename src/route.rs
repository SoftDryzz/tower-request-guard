use crate::guard::GuardConfig;
use http::Request;
use std::task::{Context, Poll};
use std::time::Duration;
use tower_layer::Layer;
use tower_service::Service;

/// Per-route override configuration. Inserted into request extensions
/// by the `route_guard` layer.
#[derive(Debug, Clone, Default)]
pub struct RouteGuardConfig {
    pub(crate) max_body_size: Option<u64>,
    pub(crate) timeout: Option<Duration>,
    pub(crate) allowed_content_types: Option<Vec<String>>,
    pub(crate) skip_headers: Vec<String>,
    pub(crate) extra_required_headers: Vec<String>,
    pub(crate) skip_all: bool,
    #[cfg(feature = "json")]
    pub(crate) max_json_depth: Option<u32>,
}

impl RouteGuardConfig {
    /// Override the maximum body size for this route.
    pub fn max_body_size(mut self, size: u64) -> Self {
        self.max_body_size = Some(size);
        self
    }

    /// Override the timeout duration for this route.
    pub fn timeout(mut self, duration: Duration) -> Self {
        self.timeout = Some(duration);
        self
    }

    /// Override the allowed Content-Type list for this route.
    pub fn allowed_content_types<I, S>(mut self, types: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.allowed_content_types = Some(types.into_iter().map(Into::into).collect());
        self
    }

    /// Skip a globally-required header for this route.
    pub fn skip_header(mut self, name: impl Into<String>) -> Self {
        self.skip_headers.push(name.into());
        self
    }

    /// Add an extra required header for this route.
    pub fn require_header(mut self, name: impl Into<String>) -> Self {
        self.extra_required_headers.push(name.into());
        self
    }

    /// Skip all validations for this route.
    pub fn skip_all(mut self) -> Self {
        self.skip_all = true;
        self
    }

    /// Override the maximum JSON depth for this route (requires `json` feature).
    #[cfg(feature = "json")]
    pub fn max_json_depth(mut self, depth: u32) -> Self {
        self.max_json_depth = Some(depth);
        self
    }

    /// Merge this route config with the global config.
    /// Route values override globals; unset values inherit from global.
    pub fn merge_with(&self, global: &GuardConfig) -> GuardConfig {
        if self.skip_all {
            return GuardConfig {
                max_body_size: None,
                timeout: None,
                allowed_content_types: None,
                required_headers: Vec::new(),
                #[cfg(feature = "json")]
                max_json_depth: None,
            };
        }

        // Required headers: start with global, remove skipped, add extras
        let mut required_headers = global.required_headers.clone();
        required_headers.retain(|h| !self.skip_headers.iter().any(|s| s.eq_ignore_ascii_case(h)));
        for extra in &self.extra_required_headers {
            if !required_headers
                .iter()
                .any(|h| h.eq_ignore_ascii_case(extra))
            {
                required_headers.push(extra.clone());
            }
        }

        GuardConfig {
            max_body_size: self.max_body_size.or(global.max_body_size),
            timeout: self.timeout.or(global.timeout),
            allowed_content_types: self
                .allowed_content_types
                .clone()
                .or_else(|| global.allowed_content_types.clone()),
            required_headers,
            #[cfg(feature = "json")]
            max_json_depth: self.max_json_depth.or(global.max_json_depth),
        }
    }
}

/// Create a per-route guard override layer.
/// The closure receives a `RouteGuardConfig` to configure route-specific overrides.
pub fn route_guard<F>(f: F) -> RouteGuardLayer
where
    F: FnOnce(RouteGuardConfig) -> RouteGuardConfig,
{
    RouteGuardLayer(f(RouteGuardConfig::default()))
}

/// Layer that inserts RouteGuardConfig into request extensions.
#[derive(Debug, Clone)]
pub struct RouteGuardLayer(RouteGuardConfig);

impl<S> Layer<S> for RouteGuardLayer {
    type Service = RouteGuardInsertService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        RouteGuardInsertService {
            inner,
            config: self.0.clone(),
        }
    }
}

/// Service that inserts RouteGuardConfig into request extensions.
#[derive(Debug, Clone)]
pub struct RouteGuardInsertService<S> {
    inner: S,
    config: RouteGuardConfig,
}

impl<S, B> Service<Request<B>> for RouteGuardInsertService<S>
where
    S: Service<Request<B>>,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = S::Future;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, mut req: Request<B>) -> Self::Future {
        req.extensions_mut().insert(self.config.clone());
        self.inner.call(req)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::guard::GuardConfig;
    use std::time::Duration;

    fn base_config() -> GuardConfig {
        GuardConfig {
            max_body_size: Some(1024),
            timeout: Some(Duration::from_secs(30)),
            allowed_content_types: Some(vec!["application/json".into()]),
            required_headers: vec!["Authorization".into(), "X-Request-Id".into()],
            #[cfg(feature = "json")]
            max_json_depth: Some(32),
        }
    }

    #[test]
    fn merge_overrides_numeric_values() {
        let route = RouteGuardConfig {
            max_body_size: Some(2048),
            timeout: Some(Duration::from_secs(60)),
            ..Default::default()
        };
        let merged = route.merge_with(&base_config());
        assert_eq!(merged.max_body_size, Some(2048));
        assert_eq!(merged.timeout, Some(Duration::from_secs(60)));
    }

    #[test]
    fn merge_replaces_content_types() {
        let route = RouteGuardConfig {
            allowed_content_types: Some(vec!["multipart/form-data".into()]),
            ..Default::default()
        };
        let merged = route.merge_with(&base_config());
        assert_eq!(
            merged.allowed_content_types,
            Some(vec!["multipart/form-data".into()])
        );
    }

    #[test]
    fn merge_skip_header_removes() {
        let route = RouteGuardConfig {
            skip_headers: vec!["Authorization".into()],
            ..Default::default()
        };
        let merged = route.merge_with(&base_config());
        assert_eq!(merged.required_headers, vec!["X-Request-Id".to_string()]);
    }

    #[test]
    fn merge_require_header_adds() {
        let route = RouteGuardConfig {
            extra_required_headers: vec!["X-Tenant-Id".into()],
            ..Default::default()
        };
        let merged = route.merge_with(&base_config());
        assert!(merged.required_headers.contains(&"X-Tenant-Id".to_string()));
        assert!(merged
            .required_headers
            .contains(&"Authorization".to_string()));
    }

    #[test]
    fn merge_skip_all_clears_everything() {
        let route = RouteGuardConfig {
            skip_all: true,
            ..Default::default()
        };
        let merged = route.merge_with(&base_config());
        assert_eq!(merged.max_body_size, None);
        assert_eq!(merged.timeout, None);
        assert!(merged.allowed_content_types.is_none());
        assert!(merged.required_headers.is_empty());
    }

    #[test]
    fn merge_inherits_unset_values() {
        let route = RouteGuardConfig::default();
        let merged = route.merge_with(&base_config());
        assert_eq!(merged.max_body_size, Some(1024));
        assert_eq!(merged.timeout, Some(Duration::from_secs(30)));
    }
}

use crate::violation::OnViolation;
use std::time::Duration;

/// Resolved guard configuration (immutable after build).
#[derive(Debug, Clone)]
pub struct GuardConfig {
    pub(crate) max_body_size: Option<u64>,
    pub(crate) timeout: Option<Duration>,
    pub(crate) allowed_content_types: Option<Vec<String>>,
    pub(crate) required_headers: Vec<String>,
    #[cfg(feature = "json")]
    pub(crate) max_json_depth: Option<u32>,
}

/// The built guard holding config and violation policy.
#[derive(Clone)]
pub struct RequestGuard {
    pub(crate) config: GuardConfig,
    pub(crate) on_violation: OnViolation,
}

impl RequestGuard {
    pub fn builder() -> RequestGuardBuilder {
        RequestGuardBuilder::default()
    }

    /// Create a Tower layer from this guard.
    pub fn layer(self) -> crate::layer::RequestGuardLayer {
        crate::layer::RequestGuardLayer::new(self)
    }
}

/// Builder for RequestGuard.
pub struct RequestGuardBuilder {
    max_body_size: Option<u64>,
    timeout: Option<Duration>,
    allowed_content_types: Option<Vec<String>>,
    required_headers: Vec<String>,
    on_violation: OnViolation,
    #[cfg(feature = "json")]
    max_json_depth: Option<u32>,
}

impl Default for RequestGuardBuilder {
    fn default() -> Self {
        Self {
            max_body_size: None,
            timeout: None,
            allowed_content_types: None,
            required_headers: Vec::new(),
            on_violation: OnViolation::default(),
            #[cfg(feature = "json")]
            max_json_depth: None,
        }
    }
}

impl RequestGuardBuilder {
    pub fn max_body_size(mut self, size: u64) -> Self {
        self.max_body_size = Some(size);
        self
    }

    pub fn timeout(mut self, duration: Duration) -> Self {
        self.timeout = Some(duration);
        self
    }

    pub fn allowed_content_types<I, S>(mut self, types: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.allowed_content_types = Some(types.into_iter().map(Into::into).collect());
        self
    }

    pub fn require_header(mut self, name: impl Into<String>) -> Self {
        self.required_headers.push(name.into());
        self
    }

    pub fn on_violation(mut self, policy: OnViolation) -> Self {
        self.on_violation = policy;
        self
    }

    #[cfg(feature = "json")]
    pub fn max_json_depth(mut self, depth: u32) -> Self {
        self.max_json_depth = Some(depth);
        self
    }

    pub fn build(self) -> RequestGuard {
        RequestGuard {
            config: GuardConfig {
                max_body_size: self.max_body_size,
                timeout: self.timeout,
                allowed_content_types: self.allowed_content_types,
                required_headers: self.required_headers,
                #[cfg(feature = "json")]
                max_json_depth: self.max_json_depth,
            },
            on_violation: self.on_violation,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn builder_defaults() {
        let guard = RequestGuard::builder().build();
        assert_eq!(guard.config.max_body_size, None);
        assert_eq!(guard.config.timeout, None);
        assert!(guard.config.allowed_content_types.is_none());
        assert!(guard.config.required_headers.is_empty());
        assert!(matches!(guard.on_violation, OnViolation::Reject));
    }

    #[test]
    fn builder_sets_all_fields() {
        let guard = RequestGuard::builder()
            .max_body_size(1_048_576)
            .timeout(Duration::from_secs(30))
            .allowed_content_types(["application/json"])
            .require_header("Authorization")
            .require_header("X-Request-Id")
            .on_violation(OnViolation::LogAndPass)
            .build();

        assert_eq!(guard.config.max_body_size, Some(1_048_576));
        assert_eq!(guard.config.timeout, Some(Duration::from_secs(30)));
        assert_eq!(
            guard.config.allowed_content_types,
            Some(vec!["application/json".to_string()])
        );
        assert_eq!(
            guard.config.required_headers,
            vec!["Authorization".to_string(), "X-Request-Id".to_string()]
        );
        assert!(matches!(guard.on_violation, OnViolation::LogAndPass));
    }

    #[test]
    fn builder_chaining() {
        let _guard = RequestGuard::builder()
            .max_body_size(1024)
            .timeout(Duration::from_secs(5))
            .require_header("Auth")
            .build();
    }
}

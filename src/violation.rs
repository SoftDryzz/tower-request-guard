use http::{Response, StatusCode};
use std::sync::Arc;

/// A request validation violation detected by the guard.
#[derive(Debug, Clone)]
pub enum Violation {
    BodyTooLarge {
        max: u64,
        received: u64,
    },
    RequestTimeout {
        timeout_ms: u64,
    },
    InvalidContentType {
        received: String,
        allowed: Vec<String>,
    },
    MissingHeader {
        header: String,
    },
    JsonTooDeep {
        max_depth: u32,
        found_depth: u32,
    },
    InvalidJson {
        detail: String,
    },
}

impl Violation {
    /// Returns the appropriate HTTP status code for this violation.
    pub fn status_code(&self) -> StatusCode {
        match self {
            Self::BodyTooLarge { .. } => StatusCode::PAYLOAD_TOO_LARGE,
            Self::RequestTimeout { .. } => StatusCode::GATEWAY_TIMEOUT,
            Self::InvalidContentType { .. } => StatusCode::UNSUPPORTED_MEDIA_TYPE,
            Self::MissingHeader { .. } => StatusCode::BAD_REQUEST,
            Self::JsonTooDeep { .. } => StatusCode::BAD_REQUEST,
            Self::InvalidJson { .. } => StatusCode::BAD_REQUEST,
        }
    }

    /// Returns the error key used in JSON responses.
    pub fn error_key(&self) -> &'static str {
        match self {
            Self::BodyTooLarge { .. } => "body_too_large",
            Self::RequestTimeout { .. } => "request_timeout",
            Self::InvalidContentType { .. } => "invalid_content_type",
            Self::MissingHeader { .. } => "missing_header",
            Self::JsonTooDeep { .. } => "json_too_deep",
            Self::InvalidJson { .. } => "invalid_json",
        }
    }
}

/// Action to take after evaluating a violation through OnViolation policy.
#[derive(Default)]
pub enum ViolationAction {
    /// Reject the request with the default error response.
    #[default]
    Reject,
    /// Let the request through (ignored for Timeout violations).
    Pass,
    /// Respond with a fully custom HTTP response.
    RespondWith(Response<String>),
}

/// Policy for handling violations.
#[derive(Clone, Default)]
pub enum OnViolation {
    /// Return the appropriate 4xx/5xx response immediately.
    #[default]
    Reject,
    /// Log the violation via tracing::warn but forward the request.
    /// Does NOT apply to Timeout violations (no response to forward).
    LogAndPass,
    /// Custom callback. Must be Fn(&Violation) -> ViolationAction + Send + Sync + 'static.
    Custom(Arc<dyn Fn(&Violation) -> ViolationAction + Send + Sync>),
}

impl OnViolation {
    /// Create a custom violation handler from a closure.
    pub fn custom<F>(f: F) -> Self
    where
        F: Fn(&Violation) -> ViolationAction + Send + Sync + 'static,
    {
        Self::Custom(Arc::new(f))
    }
}

impl std::fmt::Debug for OnViolation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Reject => write!(f, "OnViolation::Reject"),
            Self::LogAndPass => write!(f, "OnViolation::LogAndPass"),
            Self::Custom(_) => write!(f, "OnViolation::Custom(...)"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use http::StatusCode;

    #[test]
    fn violation_status_codes() {
        assert_eq!(Violation::BodyTooLarge { max: 100, received: 200 }.status_code(), StatusCode::PAYLOAD_TOO_LARGE);
        assert_eq!(Violation::RequestTimeout { timeout_ms: 5000 }.status_code(), StatusCode::GATEWAY_TIMEOUT);
        assert_eq!(
            Violation::InvalidContentType {
                received: "text/xml".into(),
                allowed: vec!["application/json".into()],
            }.status_code(),
            StatusCode::UNSUPPORTED_MEDIA_TYPE
        );
        assert_eq!(
            Violation::MissingHeader { header: "Authorization".into() }.status_code(),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            Violation::JsonTooDeep { max_depth: 32, found_depth: 128 }.status_code(),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            Violation::InvalidJson { detail: "unexpected EOF".into() }.status_code(),
            StatusCode::BAD_REQUEST
        );
    }

    #[test]
    fn on_violation_default_is_reject() {
        assert!(matches!(OnViolation::default(), OnViolation::Reject));
    }

    #[test]
    fn violation_action_default_is_reject() {
        assert!(matches!(ViolationAction::default(), ViolationAction::Reject));
    }
}

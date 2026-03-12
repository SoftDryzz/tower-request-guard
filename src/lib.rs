//! Request validation middleware for [`tower`] services.
//!
//! `tower-request-guard` validates incoming requests before they reach
//! the handler: body size limits, timeouts, content-type enforcement,
//! required headers, and JSON depth protection.

/// Body size checks and bodyless-method detection.
pub mod body;
/// Content-Type media type matching.
pub mod content_type;
/// Guard configuration and builder.
pub mod guard;
/// Required header validation.
pub mod headers;
/// Tower [`Layer`](tower_layer::Layer) implementation.
pub mod layer;
/// HTTP error response generation for violations.
pub mod response;
/// Per-route override configuration via [`route_guard`].
pub mod route;
/// Tower [`Service`](tower_service::Service) that enforces request validation.
pub mod service;
/// Violation types, actions, and handling policies.
pub mod violation;

/// JSON depth validation (requires `json` feature).
#[cfg(feature = "json")]
pub mod json;

/// Buffered body variant for JSON depth protection (requires `json` feature).
#[cfg(feature = "json")]
pub mod buffered;

// Re-exports
pub use guard::RequestGuard;
pub use layer::RequestGuardLayer;
pub use route::route_guard;
pub use service::RequestGuardService;
pub use violation::{OnViolation, Violation, ViolationAction};

#[cfg(feature = "json")]
pub use buffered::{BufferedRequestGuardLayer, BufferedRequestGuardService};

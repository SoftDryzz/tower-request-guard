//! Request validation middleware for [`tower`] services.
//!
//! `tower-request-guard` validates incoming requests before they reach
//! the handler: body size limits, timeouts, content-type enforcement,
//! required headers, and JSON depth protection.

pub mod body;
pub mod content_type;
pub mod guard;
pub mod headers;
pub mod layer;
pub mod response;
pub mod route;
pub mod service;
pub mod violation;

#[cfg(feature = "json")]
pub mod json;

#[cfg(feature = "json")]
pub mod buffered;

use http::{HeaderMap, Method};

/// Check Content-Length header against max body size.
/// Returns Some(received_size) if it exceeds the limit, None otherwise.
/// Returns None if header is absent or unparseable (stream limit handles those).
pub fn check_content_length(headers: &HeaderMap, max_body_size: u64) -> Option<u64> {
    let value = headers.get("content-length")?;
    let length: u64 = value.to_str().ok()?.parse().ok()?;
    if length > max_body_size {
        Some(length)
    } else {
        None
    }
}

/// Returns true for HTTP methods that typically carry no body.
pub fn is_bodyless_method(method: &Method) -> bool {
    matches!(
        *method,
        Method::GET | Method::HEAD | Method::DELETE | Method::OPTIONS
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use http::HeaderMap;

    #[test]
    fn content_length_within_limit() {
        let mut headers = HeaderMap::new();
        headers.insert("content-length", "500".parse().unwrap());
        assert_eq!(check_content_length(&headers, 1024), None);
    }

    #[test]
    fn content_length_exceeds_limit() {
        let mut headers = HeaderMap::new();
        headers.insert("content-length", "2048".parse().unwrap());
        assert_eq!(check_content_length(&headers, 1024), Some(2048));
    }

    #[test]
    fn content_length_exactly_at_limit() {
        let mut headers = HeaderMap::new();
        headers.insert("content-length", "1024".parse().unwrap());
        assert_eq!(check_content_length(&headers, 1024), None);
    }

    #[test]
    fn no_content_length_header() {
        let headers = HeaderMap::new();
        assert_eq!(check_content_length(&headers, 1024), None);
    }

    #[test]
    fn invalid_content_length_value() {
        let mut headers = HeaderMap::new();
        headers.insert("content-length", "not-a-number".parse().unwrap());
        assert_eq!(check_content_length(&headers, 1024), None);
    }

    #[test]
    fn is_bodyless_method_detection() {
        assert!(is_bodyless_method(&http::Method::GET));
        assert!(is_bodyless_method(&http::Method::HEAD));
        assert!(is_bodyless_method(&http::Method::DELETE));
        assert!(is_bodyless_method(&http::Method::OPTIONS));
        assert!(!is_bodyless_method(&http::Method::POST));
        assert!(!is_bodyless_method(&http::Method::PUT));
        assert!(!is_bodyless_method(&http::Method::PATCH));
    }
}

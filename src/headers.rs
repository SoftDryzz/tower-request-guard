use http::HeaderMap;

/// Check that all required headers are present.
/// Returns the first missing header name, or None if all present.
/// Lookup is case-insensitive (HeaderMap handles this).
pub fn find_missing_header(headers: &HeaderMap, required: &[String]) -> Option<String> {
    for name in required {
        if headers.get(name.as_str()).is_none() {
            return Some(name.clone());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use http::HeaderMap;

    #[test]
    fn all_headers_present() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Bearer token".parse().unwrap());
        headers.insert("x-request-id", "abc-123".parse().unwrap());
        let required = vec!["Authorization".to_string(), "X-Request-Id".to_string()];
        assert_eq!(find_missing_header(&headers, &required), None);
    }

    #[test]
    fn missing_header_detected() {
        let headers = HeaderMap::new();
        let required = vec!["Authorization".to_string()];
        assert_eq!(
            find_missing_header(&headers, &required),
            Some("Authorization".to_string())
        );
    }

    #[test]
    fn case_insensitive_lookup() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Bearer token".parse().unwrap());
        let required = vec!["Authorization".to_string()];
        assert_eq!(find_missing_header(&headers, &required), None);
    }

    #[test]
    fn returns_first_missing() {
        let mut headers = HeaderMap::new();
        headers.insert("x-request-id", "abc".parse().unwrap());
        let required = vec![
            "Authorization".to_string(),
            "X-Request-Id".to_string(),
            "X-Tenant-Id".to_string(),
        ];
        assert_eq!(
            find_missing_header(&headers, &required),
            Some("Authorization".to_string())
        );
    }

    #[test]
    fn empty_required_always_passes() {
        let headers = HeaderMap::new();
        let required: Vec<String> = vec![];
        assert_eq!(find_missing_header(&headers, &required), None);
    }
}

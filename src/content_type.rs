/// Extracts the media type from a Content-Type header value,
/// stripping parameters like charset and boundary.
fn extract_media_type(content_type: &str) -> &str {
    content_type
        .split(';')
        .next()
        .unwrap_or(content_type)
        .trim()
}

/// Check if a Content-Type value matches any of the allowed types.
/// Comparison is case-insensitive and ignores parameters.
pub fn matches_content_type(content_type: &str, allowed: &[String]) -> bool {
    let media_type = extract_media_type(content_type);
    allowed
        .iter()
        .any(|a| a.eq_ignore_ascii_case(media_type))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_match() {
        let allowed = vec!["application/json".to_string()];
        assert!(matches_content_type("application/json", &allowed));
    }

    #[test]
    fn matches_ignoring_params() {
        let allowed = vec!["application/json".to_string()];
        assert!(matches_content_type("application/json; charset=utf-8", &allowed));
    }

    #[test]
    fn matches_case_insensitive() {
        let allowed = vec!["application/json".to_string()];
        assert!(matches_content_type("Application/JSON", &allowed));
    }

    #[test]
    fn rejects_non_matching() {
        let allowed = vec!["application/json".to_string()];
        assert!(!matches_content_type("text/xml", &allowed));
    }

    #[test]
    fn multiple_allowed() {
        let allowed = vec![
            "application/json".to_string(),
            "multipart/form-data".to_string(),
        ];
        assert!(matches_content_type("multipart/form-data; boundary=abc", &allowed));
        assert!(!matches_content_type("text/plain", &allowed));
    }

    #[test]
    fn empty_allowed_rejects_all() {
        let allowed: Vec<String> = vec![];
        assert!(!matches_content_type("application/json", &allowed));
    }
}

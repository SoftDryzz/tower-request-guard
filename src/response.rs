use crate::violation::Violation;
use http::Response;

/// Escape a string for safe inclusion in a JSON string value.
/// Handles quotes, backslashes, and control characters.
pub(crate) fn escape_json_string(s: &str) -> String {
    let mut escaped = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '"' => escaped.push_str(r#"\""#),
            '\\' => escaped.push_str(r#"\\"#),
            '\n' => escaped.push_str(r#"\n"#),
            '\r' => escaped.push_str(r#"\r"#),
            '\t' => escaped.push_str(r#"\t"#),
            c if c.is_control() => {
                escaped.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => escaped.push(c),
        }
    }
    escaped
}

/// Build an HTTP error response for a given violation.
pub fn violation_response(violation: &Violation) -> Response<String> {
    let status = violation.status_code();
    let body = violation_json_body(violation);

    Response::builder()
        .status(status)
        .header("Content-Type", "application/json")
        .body(body)
        .unwrap()
}

fn violation_json_body(violation: &Violation) -> String {
    match violation {
        Violation::BodyTooLarge { max, received } => {
            format!(
                r#"{{"error":"payload too large","violation":"body_too_large","max":{},"received":{}}}"#,
                max, received
            )
        }
        Violation::RequestTimeout { timeout_ms } => {
            format!(
                r#"{{"error":"request timeout","violation":"request_timeout","timeout_ms":{}}}"#,
                timeout_ms
            )
        }
        Violation::InvalidContentType { received, allowed } => {
            let received_escaped = escape_json_string(received);
            let allowed_json: Vec<String> = allowed
                .iter()
                .map(|a| format!(r#""{}""#, escape_json_string(a)))
                .collect();
            format!(
                r#"{{"error":"unsupported content type","violation":"invalid_content_type","received":"{}","allowed":[{}]}}"#,
                received_escaped,
                allowed_json.join(",")
            )
        }
        Violation::MissingHeader { header } => {
            format!(
                r#"{{"error":"missing required header","violation":"missing_header","header":"{}"}}"#,
                escape_json_string(header)
            )
        }
        Violation::JsonTooDeep {
            max_depth,
            found_depth,
        } => {
            format!(
                r#"{{"error":"json depth exceeded","violation":"json_too_deep","max_depth":{},"found_depth":{}}}"#,
                max_depth, found_depth
            )
        }
        Violation::InvalidJson { detail } => {
            format!(
                r#"{{"error":"invalid json","violation":"invalid_json","detail":"{}"}}"#,
                escape_json_string(detail)
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::violation::Violation;
    use http::StatusCode;

    #[test]
    fn escape_json_string_handles_special_chars() {
        assert_eq!(escape_json_string(r#"hello "world""#), r#"hello \"world\""#);
        assert_eq!(escape_json_string("back\\slash"), r#"back\\slash"#);
        assert_eq!(escape_json_string("new\nline"), r#"new\nline"#);
        assert_eq!(escape_json_string("tab\there"), r#"tab\there"#);
    }

    #[test]
    fn escape_json_string_passes_through_clean_input() {
        assert_eq!(escape_json_string("application/json"), "application/json");
        assert_eq!(escape_json_string("Authorization"), "Authorization");
    }

    #[test]
    fn body_too_large_response() {
        let v = Violation::BodyTooLarge {
            max: 1024,
            received: 2048,
        };
        let resp = violation_response(&v);
        assert_eq!(resp.status(), StatusCode::PAYLOAD_TOO_LARGE);
        let body = resp.into_body();
        assert!(body.contains(r#""violation":"body_too_large""#));
        assert!(body.contains(r#""max":1024"#));
        assert!(body.contains(r#""received":2048"#));
    }

    #[test]
    fn missing_header_response() {
        let v = Violation::MissingHeader {
            header: "Authorization".into(),
        };
        let resp = violation_response(&v);
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let body = resp.into_body();
        assert!(body.contains(r#""violation":"missing_header""#));
        assert!(body.contains(r#""header":"Authorization""#));
    }

    #[test]
    fn invalid_content_type_response() {
        let v = Violation::InvalidContentType {
            received: "text/xml".into(),
            allowed: vec!["application/json".into(), "multipart/form-data".into()],
        };
        let resp = violation_response(&v);
        assert_eq!(resp.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
        let body = resp.into_body();
        assert!(body.contains(r#""violation":"invalid_content_type""#));
        assert!(body.contains(r#""received":"text/xml""#));
        assert!(body.contains(r#""allowed":["application/json","multipart/form-data"]"#));
    }

    #[test]
    fn timeout_response() {
        let v = Violation::RequestTimeout { timeout_ms: 30000 };
        let resp = violation_response(&v);
        assert_eq!(resp.status(), StatusCode::GATEWAY_TIMEOUT);
        let body = resp.into_body();
        assert!(body.contains(r#""violation":"request_timeout""#));
        assert!(body.contains(r#""timeout_ms":30000"#));
    }

    #[test]
    fn json_too_deep_response() {
        let v = Violation::JsonTooDeep {
            max_depth: 32,
            found_depth: 128,
        };
        let resp = violation_response(&v);
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let body = resp.into_body();
        assert!(body.contains(r#""violation":"json_too_deep""#));
        assert!(body.contains(r#""max_depth":32"#));
        assert!(body.contains(r#""found_depth":128"#));
    }

    #[test]
    fn invalid_json_response() {
        let v = Violation::InvalidJson {
            detail: "unexpected EOF".into(),
        };
        let resp = violation_response(&v);
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let body = resp.into_body();
        assert!(body.contains(r#""violation":"invalid_json""#));
        assert!(body.contains(r#""detail":"unexpected EOF""#));
    }

    #[test]
    fn response_escapes_untrusted_input() {
        let v = Violation::MissingHeader {
            header: r#"X-Bad"Header"#.into(),
        };
        let resp = violation_response(&v);
        let body = resp.into_body();
        assert!(body.contains(r#""header":"X-Bad\"Header""#));
    }
}

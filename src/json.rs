/// Errors from JSON depth checking.
#[derive(Debug, PartialEq)]
pub enum JsonDepthError {
    TooDeep { max_depth: u32, found_depth: u32 },
    Malformed { detail: String },
}

/// Check the maximum nesting depth of a JSON value.
/// Returns Ok(depth) if within limit, Err otherwise.
///
/// Uses serde_json::Value to parse and then walks the tree.
pub fn check_json_depth(data: &[u8], max_depth: u32) -> Result<u32, JsonDepthError> {
    let value: serde_json::Value =
        serde_json::from_slice(data).map_err(|e| JsonDepthError::Malformed {
            detail: e.to_string(),
        })?;

    let depth = measure_depth(&value);
    if depth > max_depth {
        Err(JsonDepthError::TooDeep {
            max_depth,
            found_depth: depth,
        })
    } else {
        Ok(depth)
    }
}

fn measure_depth(value: &serde_json::Value) -> u32 {
    match value {
        serde_json::Value::Object(map) => {
            let max_child = map.values().map(measure_depth).max().unwrap_or(0);
            1 + max_child
        }
        serde_json::Value::Array(arr) => {
            let max_child = arr.iter().map(measure_depth).max().unwrap_or(0);
            1 + max_child
        }
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flat_json_passes() {
        assert_eq!(check_json_depth(b"{\"a\":1,\"b\":2}", 32), Ok(1));
    }

    #[test]
    fn nested_within_limit() {
        assert_eq!(check_json_depth(b"{\"a\":{\"b\":{\"c\":1}}}", 5), Ok(3));
    }

    #[test]
    fn nested_exceeds_limit() {
        let result = check_json_depth(b"{\"a\":{\"b\":{\"c\":1}}}", 2);
        assert!(matches!(
            result,
            Err(JsonDepthError::TooDeep {
                found_depth: 3,
                max_depth: 2
            })
        ));
    }

    #[test]
    fn array_nesting_counts() {
        assert_eq!(check_json_depth(b"[[[]]]", 5), Ok(3));
    }

    #[test]
    fn mixed_nesting() {
        assert_eq!(check_json_depth(br#"{"a":[{"b":1}]}"#, 5), Ok(3));
    }

    #[test]
    fn empty_object() {
        assert_eq!(check_json_depth(b"{}", 32), Ok(1));
    }

    #[test]
    fn empty_array() {
        assert_eq!(check_json_depth(b"[]", 32), Ok(1));
    }

    #[test]
    fn scalar_value() {
        assert_eq!(check_json_depth(b"42", 32), Ok(0));
        assert_eq!(check_json_depth(b"\"hello\"", 32), Ok(0));
        assert_eq!(check_json_depth(b"true", 32), Ok(0));
        assert_eq!(check_json_depth(b"null", 32), Ok(0));
    }

    #[test]
    fn malformed_json() {
        let result = check_json_depth(b"{invalid", 32);
        assert!(matches!(result, Err(JsonDepthError::Malformed { .. })));
    }

    #[test]
    fn deeply_nested_bomb() {
        let open: String = "{\"a\":".repeat(100);
        let close: String = "}".repeat(100);
        let bomb = format!("{}1{}", open, close);
        let result = check_json_depth(bomb.as_bytes(), 32);
        assert!(matches!(result, Err(JsonDepthError::TooDeep { .. })));
    }
}

use serde_json::Value as JsonValue;

/// JSON canonicalization used by tool-call telemetry and dedup.
/// Recursively sorts object keys so semantically-equal args produce identical keys.
pub fn canonical_telemetry_args(args: &JsonValue) -> String {
    let sorted = sort_json_value(args);
    sorted.to_string()
}

fn sort_json_value(value: &JsonValue) -> JsonValue {
    match value {
        JsonValue::Array(arr) => JsonValue::Array(arr.iter().map(sort_json_value).collect()),
        JsonValue::Object(map) => {
            let mut entries: Vec<_> = map.iter().collect();
            entries.sort_by(|a, b| a.0.cmp(b.0));
            JsonValue::Object(
                entries
                    .into_iter()
                    .map(|(k, v)| (k.clone(), sort_json_value(v)))
                    .collect(),
            )
        }
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sorts_object_keys() {
        let args = serde_json::json!({ "b": 1, "a": 2 });
        assert_eq!(canonical_telemetry_args(&args), r#"{"a":2,"b":1}"#);
    }

    #[test]
    fn sorts_nested_keys() {
        let args = serde_json::json!({ "outer": { "z": true, "a": false } });
        assert_eq!(
            canonical_telemetry_args(&args),
            r#"{"outer":{"a":false,"z":true}}"#
        );
    }

    #[test]
    fn preserves_arrays() {
        let args = serde_json::json!([{ "b": 1, "a": 2 }]);
        assert_eq!(canonical_telemetry_args(&args), r#"[{"a":2,"b":1}]"#);
    }

    #[test]
    fn produces_same_key_for_equivalent_objects() {
        let a = serde_json::json!({ "x": 1, "y": { "b": 2, "a": 3 } });
        let b = serde_json::json!({ "y": { "a": 3, "b": 2 }, "x": 1 });
        assert_eq!(canonical_telemetry_args(&a), canonical_telemetry_args(&b));
    }
}

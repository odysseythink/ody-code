use serde_json::{json, Map, Value};

/// Fluent builder for a model-facing JSON Schema parameter object.
#[derive(Debug, Clone)]
pub struct InputSchema {
    value: Value,
    optional: bool,
    has_default: bool,
}

impl InputSchema {
    fn primitive(t: &str) -> Self {
        Self {
            value: json!({ "type": t }),
            optional: false,
            has_default: false,
        }
    }

    pub fn string() -> Self {
        Self::primitive("string")
    }

    pub fn number() -> Self {
        Self::primitive("number")
    }

    pub fn integer() -> Self {
        Self::primitive("integer")
    }

    pub fn boolean() -> Self {
        Self::primitive("boolean")
    }

    pub fn array(items: Self) -> Self {
        Self {
            value: json!({ "type": "array", "items": items.value }),
            optional: false,
            has_default: false,
        }
    }

    pub fn object(properties: Vec<(&str, Self)>) -> Self {
        let mut props = Map::new();
        let mut required = Vec::new();
        for (name, schema) in properties {
            if !schema.optional && !schema.has_default {
                required.push(Value::String(name.to_string()));
            }
            props.insert(name.to_string(), schema.value);
        }
        let mut value = json!({ "type": "object", "properties": props });
        if !required.is_empty() {
            value["required"] = Value::Array(required);
        }
        value["additionalProperties"] = Value::Bool(false);
        Self {
            value,
            optional: false,
            has_default: false,
        }
    }

    pub fn string_enum(values: &[&str]) -> Self {
        Self {
            value: json!({
                "type": "string",
                "enum": values.iter().map(|v| Value::String((*v).to_string())).collect::<Vec<_>>()
            }),
            optional: false,
            has_default: false,
        }
    }

    pub fn const_(value: Value) -> Self {
        Self {
            value: json!({ "const": value }),
            optional: false,
            has_default: false,
        }
    }

    pub fn record(additional_properties: Value) -> Self {
        Self {
            value: json!({
                "type": "object",
                "additionalProperties": additional_properties
            }),
            optional: false,
            has_default: false,
        }
    }

    pub fn optional(mut self) -> Self {
        self.optional = true;
        self
    }

    pub fn default(mut self, value: Value) -> Self {
        self.value["default"] = value;
        self.optional = true;
        self.has_default = true;
        self
    }

    pub fn description(mut self, desc: &str) -> Self {
        self.value["description"] = Value::String(desc.to_string());
        self
    }

    pub fn min(mut self, n: f64) -> Self {
        self.value["minimum"] = Value::from(n);
        self
    }

    pub fn max(mut self, n: f64) -> Self {
        self.value["maximum"] = Value::from(n);
        self
    }

    pub fn min_items(mut self, n: usize) -> Self {
        self.value["minItems"] = Value::from(n);
        self
    }

    pub fn max_items(mut self, n: usize) -> Self {
        self.value["maxItems"] = Value::from(n);
        self
    }

    pub fn min_length(mut self, n: usize) -> Self {
        self.value["minLength"] = Value::from(n);
        self
    }

    pub fn max_length(mut self, n: usize) -> Self {
        self.value["maxLength"] = Value::from(n);
        self
    }

    pub fn build(self) -> Value {
        self.value
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn basic_object_has_type_properties_and_closed_additional() {
        let schema = InputSchema::object(vec![
            ("name", InputSchema::string()),
            ("count", InputSchema::number().optional()),
        ])
        .build();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["name"]["type"] == "string");
        assert!(schema["properties"]["count"]["type"] == "number");
        assert_eq!(schema["additionalProperties"], false);
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&json!("name")));
        assert!(!required.contains(&json!("count")));
    }

    #[test]
    fn defaulted_field_is_not_required() {
        let schema = InputSchema::object(vec![
            ("name", InputSchema::string()),
            ("active", InputSchema::boolean().default(json!(true))),
        ])
        .build();
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&json!("name")));
        assert!(!required.contains(&json!("active")));
        assert_eq!(schema["properties"]["active"]["default"], true);
    }

    #[test]
    fn nested_objects_get_closed_additional_properties() {
        let schema = InputSchema::object(vec![(
            "nested",
            InputSchema::object(vec![("inner", InputSchema::string())]),
        )])
        .build();
        assert_eq!(
            schema["properties"]["nested"]["additionalProperties"],
            false
        );
    }

    #[test]
    fn integer_with_bounds_and_description() {
        let schema = InputSchema::integer()
            .min(1.0)
            .max(100.0)
            .default(json!(20))
            .description("A bounded integer")
            .build();
        assert_eq!(schema["type"], "integer");
        assert_eq!(schema["minimum"], 1.0);
        assert_eq!(schema["maximum"], 100.0);
        assert_eq!(schema["default"], 20);
        assert_eq!(schema["description"], "A bounded integer");
    }

    #[test]
    fn array_with_items_bounds_and_description() {
        let schema = InputSchema::array(InputSchema::string())
            .min_items(1)
            .max_items(4)
            .description("A bounded list")
            .build();
        assert_eq!(schema["type"], "array");
        assert_eq!(schema["items"]["type"], "string");
        assert_eq!(schema["minItems"], 1);
        assert_eq!(schema["maxItems"], 4);
        assert_eq!(schema["description"], "A bounded list");
    }

    #[test]
    fn string_enum_renders_enum_values() {
        let schema = InputSchema::string_enum(&["read", "write", "search"]).build();
        assert_eq!(schema["type"], "string");
        let values = schema["enum"].as_array().unwrap();
        assert_eq!(
            values,
            &vec![json!("read"), json!("write"), json!("search")]
        );
    }

    #[test]
    fn const_renders_const_value() {
        let schema = InputSchema::const_(json!("always")).build();
        assert_eq!(schema["const"], "always");
    }
}

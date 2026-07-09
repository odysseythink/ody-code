use jsonschema::{Draft, Validator};
use serde_json::Value;

pub type ToolArgsValidator = Validator;

const DRAFT_2019_KEYWORDS: &[&str] = &[
    "dependentRequired",
    "dependentSchemas",
    "maxContains",
    "minContains",
    "unevaluatedItems",
    "unevaluatedProperties",
    "$recursiveAnchor",
    "$recursiveRef",
];

const DRAFT_2020_KEYWORDS: &[&str] = &["prefixItems", "$dynamicAnchor", "$dynamicRef"];

pub fn compile_tool_args_validator(
    schema: &Value,
) -> Result<ToolArgsValidator, jsonschema::ValidationError<'static>> {
    let draft = select_draft(schema);
    jsonschema::options().with_draft(draft).build(schema)
}

fn select_draft(schema: &Value) -> Draft {
    if let Some(Value::String(schema_url)) = schema.get("$schema") {
        if schema_url.contains("2020-12") {
            return Draft::Draft202012;
        }
        if schema_url.contains("2019-09") {
            return Draft::Draft201909;
        }
        return Draft::Draft7;
    }
    if contains_schema_keyword(schema, DRAFT_2020_KEYWORDS) {
        Draft::Draft202012
    } else if contains_schema_keyword(schema, DRAFT_2019_KEYWORDS) {
        Draft::Draft201909
    } else {
        Draft::Draft7
    }
}

fn contains_schema_keyword(value: &Value, keywords: &[&str]) -> bool {
    match value {
        Value::Array(arr) => arr
            .iter()
            .any(|item| contains_schema_keyword(item, keywords)),
        Value::Object(map) => map.iter().any(|(key, child)| {
            keywords.contains(&key.as_str()) || contains_schema_keyword(child, keywords)
        }),
        _ => false,
    }
}

pub fn validate_tool_args(validator: &ToolArgsValidator, args: &Value) -> Option<String> {
    let errors: Vec<String> = validator
        .iter_errors(args)
        .map(format_validation_error)
        .collect();
    if errors.is_empty() {
        None
    } else {
        Some(errors.join("; "))
    }
}

fn format_validation_error(error: jsonschema::ValidationError<'_>) -> String {
    use jsonschema::error::ValidationErrorKind;
    match &error.kind {
        ValidationErrorKind::Required { property } => {
            let name = property.as_str().unwrap_or("");
            format!("must have required property '{}'", name)
        }
        ValidationErrorKind::AdditionalProperties { unexpected } => {
            let name = unexpected.first().cloned().unwrap_or_default();
            format!("must NOT have additional property '{}'", name)
        }
        ValidationErrorKind::UnevaluatedProperties { unexpected } => {
            let name = unexpected.first().cloned().unwrap_or_default();
            format!("must NOT have additional property '{}'", name)
        }
        _ => {
            let path = error.instance_path.to_string();
            let message = error.to_string();
            if path.is_empty() {
                message
            } else {
                format!("{} {}", path, message)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::InputSchema;
    use serde_json::json;

    fn task_list_schema() -> Value {
        InputSchema::object(vec![
            ("active_only", InputSchema::boolean().default(json!(true))),
            (
                "limit",
                InputSchema::integer()
                    .min(1.0)
                    .max(100.0)
                    .default(json!(20))
                    .optional(),
            ),
        ])
        .build()
    }

    #[test]
    fn empty_object_passes_with_defaults() {
        let validator = compile_tool_args_validator(&task_list_schema()).unwrap();
        assert_eq!(validate_tool_args(&validator, &json!({})), None);
    }

    #[test]
    fn valid_arguments_pass() {
        let validator = compile_tool_args_validator(&task_list_schema()).unwrap();
        assert_eq!(
            validate_tool_args(&validator, &json!({"active_only": false, "limit": 10})),
            None
        );
    }

    #[test]
    fn missing_required_property_reports_ajv_message() {
        let schema = InputSchema::object(vec![
            ("name", InputSchema::string()),
            ("count", InputSchema::integer().optional()),
        ])
        .build();
        let validator = compile_tool_args_validator(&schema).unwrap();
        let error = validate_tool_args(&validator, &json!({"count": 5})).unwrap();
        assert!(error.contains("must have required property 'name'"));
    }

    #[test]
    fn additional_top_level_property_reports_ajv_message() {
        let validator = compile_tool_args_validator(&task_list_schema()).unwrap();
        let error = validate_tool_args(&validator, &json!({"bogus": true})).unwrap();
        assert!(error.contains("must NOT have additional property 'bogus'"));
    }

    #[test]
    fn additional_nested_property_reports_ajv_message() {
        let schema = InputSchema::object(vec![(
            "question",
            InputSchema::object(vec![
                ("question", InputSchema::string()),
                (
                    "options",
                    InputSchema::array(InputSchema::string()).min_items(2),
                ),
            ]),
        )])
        .build();
        let validator = compile_tool_args_validator(&schema).unwrap();
        let args = json!({
            "question": {
                "question": "Which?",
                "options": ["A", "B"],
                "bogus": true
            }
        });
        let error = validate_tool_args(&validator, &args).unwrap();
        assert!(error.contains("must NOT have additional property 'bogus'"));
    }

    #[test]
    fn selects_draft_2020_12_by_schema_keyword() {
        let schema = json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "properties": {
                "items": { "type": "array", "prefixItems": [{ "type": "string" }] }
            },
            "additionalProperties": false
        });
        let validator = compile_tool_args_validator(&schema).unwrap();
        assert_eq!(
            validate_tool_args(&validator, &json!({"items": ["ok"]})),
            None
        );
    }
}

# Part 3 — Input JSON Schema Builder and Args Validator

**Goal:** Provide Rust equivalents of `toInputJsonSchema` (a builder that emits model-facing JSON Schema with defaulted fields optional and `additionalProperties: false` everywhere) and the AJV-style args validator used at tool-call time.

**Architecture:** `schema.rs` defines a small fluent builder that mirrors the zod-to-JSON-Schema shapes actually used by builtin tools (object/string/number/integer/boolean/array/enum/const). `args_validator.rs` wraps the `jsonschema` crate, selects the draft by `$schema`/keyword heuristics, and normalizes the two messages the rest of the system depends on (`must have required property`, `must NOT have additional property`). Both are pure value transformations with no async I/O.

**Tech Stack:** Rust 2021, `serde_json`, `jsonschema`.

> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task — a clean context per task avoids single-session degradation). Steps use - [ ] checkboxes for tracking.

---

## File Structure

| Responsibility | Path |
|---|---|
| JSON Schema builder | `rust-ody/crates/tools-rs/src/schema.rs` |
| Args validator | `rust-ody/crates/tools-rs/src/args_validator.rs` |
| Crate public export | `rust-ody/crates/tools-rs/src/lib.rs` |

---

## Dependency Overview

```
Task 6  Input JSON-schema builder
   │
   └──► Task 7  Args validator (consumes the schemas built in Task 6)
```

- Task 6 depends only on Task 1 (crate exists).
- Task 7 depends on Task 6 (it validates schemas; tests reuse builder output).
- Both tasks are independent of path-policy/support helpers.

---

## Risks & Open Questions

| Risk | Mitigation |
|---|---|
| `jsonschema` error wording differs from AJV for non-required/additional errors | Normalize only the two messages the TS runtime relies on; other errors keep `jsonschema`'s default wording and are covered by L1 fixtures. |
| Builder may miss zod features used by future tools | Scope Task 6 to the shapes found in current builtin tools (object, string, number, integer, boolean, array, enum, const, default, optional, descriptions, numeric/array bounds). |

---

## Task 6: Input JSON-schema builder

**Depends on:** Part 1 Task 1 (`tools-rs` crate exists)  
**Files:**
- Create: `rust-ody/crates/tools-rs/src/schema.rs`
- Modify: `rust-ody/crates/tools-rs/src/lib.rs`

**Goal:** Port the `toInputJsonSchema` helper as a Rust builder so builtin tools can declaratively construct the JSON Schema they advertise to the model.

### Steps

- [ ] Write the failing test first in `rust-ody/crates/tools-rs/src/schema.rs`:

```rust
#[cfg(test)]
mod tests {
    use serde_json::json;
    use super::*;

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
        let schema = InputSchema::object(vec![
            (
                "nested",
                InputSchema::object(vec![("inner", InputSchema::string())]),
            ),
        ])
        .build();
        assert_eq!(schema["properties"]["nested"]["additionalProperties"], false);
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
        assert_eq!(values, &vec![json!("read"), json!("write"), json!("search")]);
    }

    #[test]
    fn const_renders_const_value() {
        let schema = InputSchema::const_(json!("always")).build();
        assert_eq!(schema["const"], "always");
    }
}
```

- [ ] Run the test and confirm it fails because the module is not exposed:

```bash
cd rust-ody && cargo test -p tools-rs schema::tests
```

Expected failure: `cannot find module `schema` in module `tools_rs`` or similar.

- [ ] Create `rust-ody/crates/tools-rs/src/schema.rs`:

```rust
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
```

- [ ] Update `rust-ody/crates/tools-rs/src/lib.rs` to expose the new module:

```rust
pub mod args_validator;
pub mod policies;
pub mod result_builder;
pub mod schema;
pub mod store;
pub mod tool_accesses;
pub mod types;
pub mod workspace;
```

- [ ] Run the tests:

```bash
cd rust-ody && cargo test -p tools-rs schema::tests
```

Expected: `test result: ok.` for all `schema::tests`.

- [ ] Commit:

```bash
git add rust-ody/crates/tools-rs/src

git commit -m "feat(tools-rs): input JSON-schema builder"
```

---

## Task 7: Args validator

**Depends on:** Task 6 (uses builder output in tests; no code dependency)  
**Files:**
- Create: `rust-ody/crates/tools-rs/src/args_validator.rs`
- Modify: `rust-ody/crates/tools-rs/src/lib.rs` (already updated in Task 6)

**Goal:** Port `compileToolArgsValidator` and `validateToolArgs` so Rust builtin tools can validate incoming JSON arguments with AJV-compatible error messages.

### Steps

- [ ] Write the failing tests first in `rust-ody/crates/tools-rs/src/args_validator.rs`:

```rust
#[cfg(test)]
mod tests {
    use serde_json::json;
    use super::*;
    use crate::schema::InputSchema;

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
                ("options", InputSchema::array(InputSchema::string()).min_items(2)),
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
        assert_eq!(validate_tool_args(&validator, &json!({"items": ["ok"]})), None);
    }
}
```

- [ ] Run the test and confirm it fails because the module is not implemented:

```bash
cd rust-ody && cargo test -p tools-rs args_validator::tests
```

Expected failure: `cannot find module `args_validator` in module `tools_rs`` or similar.

- [ ] Create `rust-ody/crates/tools-rs/src/args_validator.rs`:

```rust
use std::collections::HashSet;

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

pub fn compile_tool_args_validator(schema: &Value) -> Result<ToolArgsValidator, jsonschema::ValidationError<'static>> {
    let draft = select_draft(schema);
    jsonschema::options().with_draft(draft).build(schema)
}

fn select_draft(schema: &Value) -> Draft {
    if let Some(Value::String($schema)) = schema.get("$schema") {
        if $schema.contains("2020-12") {
            return Draft::Draft202012;
        }
        if $schema.contains("2019-09") {
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
        Value::Array(arr) => arr.iter().any(|item| contains_schema_keyword(item, keywords)),
        Value::Object(map) => map.iter().any(|(key, child)| {
            keywords.contains(&key.as_str()) || contains_schema_keyword(child, keywords)
        }),
        _ => false,
    }
}

pub fn validate_tool_args(validator: &ToolArgsValidator, args: &Value) -> Option<String> {
    let errors: Vec<String> = validator.iter_errors(args).map(format_validation_error).collect();
    if errors.is_empty() {
        None
    } else {
        Some(errors.join("; "))
    }
}

fn format_validation_error(error: jsonschema::ValidationError<'_>) -> String {
    use jsonschema::ValidationErrorKind;
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
            let message = error.kind.to_string();
            if path.is_empty() {
                message
            } else {
                format!("{} {}", path, message)
            }
        }
    }
}
```

- [ ] Update `rust-ody/crates/tools-rs/src/lib.rs` (if not already done in Task 6):

```rust
pub mod args_validator;
pub mod policies;
pub mod result_builder;
pub mod schema;
pub mod store;
pub mod tool_accesses;
pub mod types;
pub mod workspace;
```

- [ ] Run the tests:

```bash
cd rust-ody && cargo test -p tools-rs args_validator::tests
```

Expected: `test result: ok.` for all `args_validator::tests`.

- [ ] Commit:

```bash
git add rust-ody/crates/tools-rs/src

git commit -m "feat(tools-rs): AJV-style args validator"
```

---

## Local Self-Review (Part 3)

- [ ] 1. Spec-coverage table:

| Spec item | Task(s) | Status |
|---|---|---|
| 4.4.0 — Input JSON Schema builder (`toInputJsonSchema` equivalent) | Task 6 | covered |
| 4.4.0 — `additionalProperties: false` on object nodes | Task 6 | covered |
| 4.4.0 — Defaulted fields stay optional in input schema | Task 6 | covered |
| 4.4.0 — Args validator with AJV-style messages | Task 7 | covered |
| 4.4.0 — Draft selection by `$schema` / keyword heuristics | Task 7 | covered |
| 4.4.0 — L1 parity fixtures for schemas/validation | Part 5 (fixtures-ci.md) | downstream |

- [ ] 2. Placeholder scan: no TODO/TBD; every builder method and validator path is implemented.
- [ ] 3. No phantom tasks: each task creates source files, updates `lib.rs`, and ends with passing `cargo test`.
- [ ] 4. Dependency soundness: Task 6 depends on Task 1; Task 7 depends on Task 6 for test schemas but does not import symbols defined later.
- [ ] 5. Caller & build soundness: Part 3 only adds new modules; no existing signatures are changed, so no caller updates are required.
- [ ] 6. Test-the-risk:
  - Schema builder asserts `additionalProperties: false`, required/optional semantics, and nested object closure.
  - Args validator asserts required-property and additional-property messages, nested closure, and draft-2020-12 selection.
- [ ] 7. Type consistency: `compile_tool_args_validator` accepts `serde_json::Value`, the same type `InputSchema::build()` returns.

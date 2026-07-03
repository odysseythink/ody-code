# Part 1: Shared groundwork

本部分完成三家 OpenAI-Compatible provider 的公共前置：capability registry 扩展、`chat_completions_stream` 的 usage extractor 扩展点（供 Kimi `choices[0].usage` 使用），以及 Kimi 工具参数归一化 `kimi-schema`。

---

### Task 1: Capability registry 扩展

**Depends on:** none（复用 4.2.1/4.2.2 已存在的 `capability_registry.rs` 结构）

**Files:**
- Modify: `rust-ody/crates/kosong-rs/src/capability_registry.rs:1-256`
- Test: `rust-ody/crates/kosong-rs/src/capability_registry.rs`（新增 `#[cfg(test)]` 用例）

**实现步骤：**

- [ ] 先写测试，覆盖三家 capability 查询：

```rust
#[cfg(test)]
mod compatibility_tests {
    use super::*;

    #[test]
    fn kimi_k2_thinks_and_uses_tools() {
        let cap = get_kimi_model_capability("kimi-k2-0711");
        assert!(cap.thinking);
        assert!(cap.tool_use);
        assert!(!cap.image_in);
        assert!(!cap.video_in);
        assert!(!cap.audio_in);
    }

    #[test]
    fn kimi_unknown_is_unknown() {
        let cap = get_kimi_model_capability("kimi-unknown");
        assert!(cap.is_unknown());
    }

    #[test]
    fn deepseek_reasoner_thinks_no_tools() {
        let cap = get_deepseek_model_capability("deepseek-reasoner");
        assert!(cap.thinking);
        assert!(!cap.tool_use);
    }

    #[test]
    fn deepseek_chat_uses_tools_no_thinking() {
        let cap = get_deepseek_model_capability("deepseek-chat");
        assert!(!cap.thinking);
        assert!(cap.tool_use);
    }

    #[test]
    fn deepseek_v4_thinks_and_uses_tools_with_context() {
        let cap = get_deepseek_model_capability("deepseek-v4-0320");
        assert!(cap.thinking);
        assert!(cap.tool_use);
        assert_eq!(cap.max_context_tokens, 1_000_000);
        assert_eq!(cap.max_output_tokens, 384_000);
    }

    #[test]
    fn glm_always_unknown() {
        let cap = get_glm_model_capability("glm-4-flash");
        assert!(cap.is_unknown());
    }
}
```

运行测试并确认失败：

```bash
cd rust-ody && cargo test -p kosong-rs capability_registry::compatibility_tests
```

预期失败：`get_kimi_model_capability` / `get_deepseek_model_capability` / `get_glm_model_capability` 未定义。

- [ ] 在 `capability_registry.rs` 实现 capability 函数：

```rust
fn kimi_k2_capability() -> ModelCapability {
    ModelCapability {
        image_in: false,
        video_in: false,
        audio_in: false,
        thinking: true,
        tool_use: true,
        max_context_tokens: 0,
        max_output_tokens: 0,
    }
}

fn deepseek_reasoner_capability() -> ModelCapability {
    ModelCapability {
        image_in: false,
        video_in: false,
        audio_in: false,
        thinking: true,
        tool_use: false,
        max_context_tokens: 0,
        max_output_tokens: 0,
    }
}

fn deepseek_chat_capability() -> ModelCapability {
    ModelCapability {
        image_in: false,
        video_in: false,
        audio_in: false,
        thinking: false,
        tool_use: true,
        max_context_tokens: 0,
        max_output_tokens: 0,
    }
}

fn deepseek_v4_capability() -> ModelCapability {
    ModelCapability {
        image_in: false,
        video_in: false,
        audio_in: false,
        thinking: true,
        tool_use: true,
        max_context_tokens: 1_000_000,
        max_output_tokens: 384_000,
    }
}

pub fn get_kimi_model_capability(model_name: &str) -> ModelCapability {
    let normalized = normalize_model_name(model_name);
    if normalized.starts_with("kimi-k2") {
        return kimi_k2_capability();
    }
    ModelCapability::unknown()
}

pub fn get_deepseek_model_capability(model_name: &str) -> ModelCapability {
    let normalized = normalize_model_name(model_name);
    if normalized.starts_with("deepseek-reasoner") {
        return deepseek_reasoner_capability();
    }
    if normalized.starts_with("deepseek-v4-") {
        return deepseek_v4_capability();
    }
    if normalized.starts_with("deepseek-chat") {
        return deepseek_chat_capability();
    }
    ModelCapability::unknown()
}

pub fn get_glm_model_capability(_model_name: &str) -> ModelCapability {
    ModelCapability::unknown()
}
```

- [ ] 重新运行测试：

```bash
cd rust-ody && cargo test -p kosong-rs capability_registry::compatibility_tests
```

预期：全部通过。

- [ ] Commit: `feat(kosong-rs): add kimi/deepseek/glm capability lookups`

---

### Task 2: `chat_completions_stream` usage extractor 扩展点

**Depends on:** Task 1（无真正依赖，但属于同一 Phase）

**Files:**
- Modify: `rust-ody/crates/kosong-rs/src/chat_completions_stream.rs:51-67`
- Test: `rust-ody/crates/kosong-rs/src/chat_completions_stream.rs`（新增 `#[cfg(test)]` 用例）

**实现步骤：**

- [ ] 先写测试，覆盖默认行为与 Kimi 专用 `choices[0].usage`：

```rust
#[cfg(test)]
mod usage_extractor_tests {
    use super::*;

    #[tokio::test]
    async fn default_extractor_prefers_top_level_usage() {
        let sse = r#"data: {"id":"1","choices":[{"index":0,"delta":{"content":"hi"}}],"usage":{"prompt_tokens":10,"completion_tokens":2,"cached_tokens":3}}

data: [DONE]

"#;
        let (parts, _, usage, _, _) = parse_stream_response(sse.into(), None).await.unwrap();
        let u = usage.unwrap();
        assert_eq!(u.input_other, 7);
        assert_eq!(u.input_cache_read, 3);
        assert_eq!(u.output, 2);
    }

    #[tokio::test]
    async fn kimi_extractor_reads_choice_usage() {
        let sse = r#"data: {"id":"1","choices":[{"index":0,"delta":{"content":"hi"},"usage":{"prompt_tokens":20,"completion_tokens":4,"cached_tokens":5}}]}

data: [DONE]

"#;
        let (parts, _, usage, _, _) = parse_stream_response_with_usage_extractor(
            sse.into(),
            None,
            kimi_usage_extractor,
        )
        .await
        .unwrap();
        let u = usage.unwrap();
        assert_eq!(u.input_other, 15);
        assert_eq!(u.input_cache_read, 5);
        assert_eq!(u.output, 4);
    }

    #[tokio::test]
    async fn top_level_usage_wins_over_choice_usage() {
        let sse = r#"data: {"id":"1","choices":[{"index":0,"delta":{"content":"hi"},"usage":{"prompt_tokens":20,"completion_tokens":4,"cached_tokens":5}}],"usage":{"prompt_tokens":100,"completion_tokens":10,"cached_tokens":30}}

data: [DONE]

"#;
        let (parts, _, usage, _, _) = parse_stream_response_with_usage_extractor(
            sse.into(),
            None,
            kimi_usage_extractor,
        )
        .await
        .unwrap();
        let u = usage.unwrap();
        assert_eq!(u.input_other, 70); // top-level wins
        assert_eq!(u.input_cache_read, 30);
        assert_eq!(u.output, 10);
    }
}

fn kimi_usage_extractor(value: &serde_json::Value) -> Option<serde_json::Value> {
    value
        .get("choices")
        .and_then(|c| c.as_array())
        .and_then(|arr| arr.first())
        .and_then(|c| c.get("usage"))
        .cloned()
}
```

运行测试并确认失败：

```bash
cd rust-ody && cargo test -p kosong-rs chat_completions_stream::usage_extractor_tests
```

预期失败：`parse_stream_response_with_usage_extractor` / `kimi_usage_extractor` 未定义。

- [ ] 实现扩展函数（保留既有 `parse_stream_response` 签名不变）：

```rust
pub async fn parse_stream_response_with_usage_extractor(
    body: Vec<u8>,
    reasoning_key: Option<&str>,
    usage_extractor: fn(&serde_json::Value) -> Option<Value>,
) -> Result<(Vec<StreamedMessagePart>, Option<String>, Option<TokenUsage>, Option<FinishReason>, Option<String>), ChatProviderError> {
    let text = String::from_utf8_lossy(&body);
    let mut parts = Vec::new();
    let mut id: Option<String> = None;
    let mut usage: Option<TokenUsage> = None;
    let mut finish_reason: Option<FinishReason> = None;
    let mut raw_finish_reason: Option<String> = None;
    let mut buffered_tool_calls: HashMap<String, BufferedChatCompletionToolCall> = HashMap::new();

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || !line.starts_with("data: ") {
            continue;
        }
        let data = &line[6..];
        if data == "[DONE]" {
            break;
        }
        let chunk: ChatCompletionChunk = match serde_json::from_str(data) {
            Ok(c) => c,
            Err(_) => continue,
        };
        if let Some(cid) = chunk.id {
            id = Some(cid);
        }

        // Default top-level usage first; fall back to custom extractor.
        let raw_usage = chunk.usage.clone().or_else(|| {
            usage_extractor(&serde_json::to_value(&chunk).unwrap())
        });
        if let Some(u) = raw_usage {
            usage = extract_usage(&u);
        }

        for choice in chunk.choices {
            if let Some(raw) = &choice.finish_reason {
                let (fr, rfr) = normalize_openai_finish_reason(Some(raw.as_str()));
                finish_reason = fr.or(finish_reason);
                raw_finish_reason = rfr.or(raw_finish_reason);
            }
            let dv = serde_json::to_value(&choice.delta).unwrap();
            if let Some(reasoning) = extract_reasoning_content(&dv, reasoning_key) {
                parts.push(StreamedMessagePart::Content(ContentPart::Think {
                    think: reasoning,
                    encrypted: None,
                }));
            }
            if let Some(content) = choice.delta.content {
                parts.push(StreamedMessagePart::text(content));
            }
            if let Some(tool_calls) = choice.delta.tool_calls {
                for tc in tool_calls {
                    parts.extend(convert_chat_completion_stream_tool_call(&tc, &mut buffered_tool_calls));
                }
            }
        }
    }

    Ok((parts, id, usage, finish_reason, raw_finish_reason))
}

pub async fn parse_stream_response(
    body: Vec<u8>,
    reasoning_key: Option<&str>,
) -> Result<(Vec<StreamedMessagePart>, Option<String>, Option<TokenUsage>, Option<FinishReason>, Option<String>), ChatProviderError> {
    parse_stream_response_with_usage_extractor(body, reasoning_key, |_value| None).await
}
```

- [ ] 运行测试：

```bash
cd rust-ody && cargo test -p kosong-rs chat_completions_stream::usage_extractor_tests
```

预期：全部通过。

- [ ] 运行整个 crate 测试确认未破坏既有 OpenAI 解析：

```bash
cd rust-ody && cargo test -p kosong-rs chat_completions_stream
```

- [ ] Commit: `feat(kosong-rs): add usage extractor hook for chat-completions stream parser`

---

### Task 3: Kimi 工具参数归一化（`kimi-schema`）

**Depends on:** none（纯函数，不依赖前两个 Task）

**Files:**
- Create: `rust-ody/crates/kosong-rs/src/kimi_schema.rs:1-450`
- Modify: `rust-ody/crates/kosong-rs/src/lib.rs:1-33`（导出模块与函数）
- Test: `rust-ody/crates/kosong-rs/src/kimi_schema.rs`（内联 `#[cfg(test)]`）

**实现步骤：**

- [ ] 创建 `rust-ody/crates/kosong-rs/src/kimi_schema.rs`，先写测试：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infers_string_type_for_plain_property() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "name": { "description": "a name" }
            }
        });
        let out = normalize_kimi_tool_schema(schema.as_object().unwrap().clone());
        assert_eq!(out["properties"]["name"]["type"], "string");
    }

    #[test]
    fn infers_object_type_from_structure() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "nested": {
                    "properties": { "x": { "type": "string" } },
                    "required": ["x"]
                }
            }
        });
        let out = normalize_kimi_tool_schema(schema.as_object().unwrap().clone());
        assert_eq!(out["properties"]["nested"]["type"], "object");
    }

    #[test]
    fn infers_type_from_enum_values() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "level": { "enum": ["low", "medium", "high"] }
            }
        });
        let out = normalize_kimi_tool_schema(schema.as_object().unwrap().clone());
        assert_eq!(out["properties"]["level"]["type"], "string");
    }

    #[test]
    fn derefs_local_defs() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "refProp": { "$ref": "#/$defs/Node" }
            },
            "$defs": {
                "Node": { "type": "object", "properties": { "id": { "type": "string" } } }
            }
        });
        let out = normalize_kimi_tool_schema(schema.as_object().unwrap().clone());
        assert_eq!(out["properties"]["refProp"]["type"], "object");
        assert!(out["$defs"].is_null() || out["$defs"].as_object().unwrap().is_empty());
    }

    #[test]
    fn preserves_circular_refs_and_definition_bucket() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "self": { "$ref": "#/$defs/Self" }
            },
            "$defs": {
                "Self": { "type": "object", "properties": { "self": { "$ref": "#/$defs/Self" } } }
            }
        });
        let out = normalize_kimi_tool_schema(schema.as_object().unwrap().clone());
        assert_eq!(out["properties"]["self"]["$ref"], "#/$defs/Self");
        assert!(out["$defs"].is_object());
    }
}
```

运行测试并确认失败：

```bash
cd rust-ody && cargo test -p kosong-rs kimi_schema::tests
```

预期失败：模块/函数未定义。

- [ ] 实现归一化器：

```rust
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};

const TYPE_COMPLETION_SKIP_KEYS: &[&str] = &[
    "$ref", "allOf", "anyOf", "else", "if", "not", "oneOf", "then",
];

#[derive(Debug, Clone, Copy)]
enum SchemaSlotKind { Single, Array, Map, SchemaOrArray }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StructuralType { String, Object, Array }

#[derive(Debug, Clone, Copy)]
struct ChildSchemaSlot {
    key: &'static str,
    kind: SchemaSlotKind,
    parent_type: Option<StructuralType>,
}

const CHILD_SCHEMA_SLOTS: &[ChildSchemaSlot] = &[
    ChildSchemaSlot { key: "$defs", kind: SchemaSlotKind::Map, parent_type: None },
    ChildSchemaSlot { key: "definitions", kind: SchemaSlotKind::Map, parent_type: None },
    ChildSchemaSlot { key: "dependencies", kind: SchemaSlotKind::Map, parent_type: Some(StructuralType::Object) },
    ChildSchemaSlot { key: "dependentSchemas", kind: SchemaSlotKind::Map, parent_type: Some(StructuralType::Object) },
    ChildSchemaSlot { key: "patternProperties", kind: SchemaSlotKind::Map, parent_type: Some(StructuralType::Object) },
    ChildSchemaSlot { key: "properties", kind: SchemaSlotKind::Map, parent_type: Some(StructuralType::Object) },
    ChildSchemaSlot { key: "additionalItems", kind: SchemaSlotKind::Single, parent_type: Some(StructuralType::Array) },
    ChildSchemaSlot { key: "additionalProperties", kind: SchemaSlotKind::Single, parent_type: Some(StructuralType::Object) },
    ChildSchemaSlot { key: "contains", kind: SchemaSlotKind::Single, parent_type: Some(StructuralType::Array) },
    ChildSchemaSlot { key: "contentSchema", kind: SchemaSlotKind::Single, parent_type: Some(StructuralType::String) },
    ChildSchemaSlot { key: "else", kind: SchemaSlotKind::Single, parent_type: None },
    ChildSchemaSlot { key: "if", kind: SchemaSlotKind::Single, parent_type: None },
    ChildSchemaSlot { key: "not", kind: SchemaSlotKind::Single, parent_type: None },
    ChildSchemaSlot { key: "propertyNames", kind: SchemaSlotKind::Single, parent_type: Some(StructuralType::Object) },
    ChildSchemaSlot { key: "then", kind: SchemaSlotKind::Single, parent_type: None },
    ChildSchemaSlot { key: "unevaluatedItems", kind: SchemaSlotKind::Single, parent_type: Some(StructuralType::Array) },
    ChildSchemaSlot { key: "unevaluatedProperties", kind: SchemaSlotKind::Single, parent_type: Some(StructuralType::Object) },
    ChildSchemaSlot { key: "allOf", kind: SchemaSlotKind::Array, parent_type: None },
    ChildSchemaSlot { key: "anyOf", kind: SchemaSlotKind::Array, parent_type: None },
    ChildSchemaSlot { key: "oneOf", kind: SchemaSlotKind::Array, parent_type: None },
    ChildSchemaSlot { key: "prefixItems", kind: SchemaSlotKind::Array, parent_type: Some(StructuralType::Array) },
    ChildSchemaSlot { key: "items", kind: SchemaSlotKind::SchemaOrArray, parent_type: Some(StructuralType::Array) },
];

fn child_schema_keys_for_parent_type(parent_type: StructuralType) -> Vec<&'static str> {
    CHILD_SCHEMA_SLOTS
        .iter()
        .filter_map(|slot| {
            if slot.parent_type == Some(parent_type) {
                Some(slot.key)
            } else {
                None
            }
        })
        .collect()
}

fn object_structure_keys() -> HashSet<&'static str> {
    let mut set: HashSet<&'static str> = child_schema_keys_for_parent_type(StructuralType::Object).into_iter().collect();
    set.extend(["dependentRequired", "maxProperties", "minProperties", "required"]);
    set
}

fn array_structure_keys() -> HashSet<&'static str> {
    let mut set: HashSet<&'static str> = child_schema_keys_for_parent_type(StructuralType::Array).into_iter().collect();
    set.extend(["maxContains", "maxItems", "minContains", "minItems", "uniqueItems"]);
    set
}

fn string_structure_keys() -> HashSet<&'static str> {
    let mut set: HashSet<&'static str> = child_schema_keys_for_parent_type(StructuralType::String).into_iter().collect();
    set.extend(["contentEncoding", "contentMediaType", "format", "maxLength", "minLength", "pattern"]);
    set
}

fn numeric_structure_keys() -> HashSet<&'static str> {
    ["exclusiveMaximum", "exclusiveMinimum", "maximum", "minimum", "multipleOf"].into_iter().collect()
}

pub fn normalize_kimi_tool_schema(schema: Map<String, Value>) -> Map<String, Value> {
    ensure_kimi_property_types(deref_json_schema(schema))
}

fn deref_json_schema(schema: Map<String, Value>) -> Map<String, Value> {
    let mut visited = HashSet::new();
    let result = resolve_node(Value::Object(schema), &schema, &mut visited);
    let mut result = match result {
        Value::Object(m) => m,
        _ => panic!("JSON Schema root must normalize to an object"),
    };
    if !has_unresolved_definition_ref(&result, "$defs") {
        result.remove("$defs");
    }
    if !has_unresolved_definition_ref(&result, "definitions") {
        result.remove("definitions");
    }
    result
}

fn has_unresolved_definition_ref(node: &Value, bucket_key: &str) -> bool {
    match node {
        Value::Array(arr) => arr.iter().any(|c| has_unresolved_definition_ref(c, bucket_key)),
        Value::Object(obj) => {
            if let Some(Value::String(r)) = obj.get("$ref") {
                if r.starts_with(&format!("#/{bucket_key}/")) {
                    return true;
                }
            }
            obj.iter().any(|(k, v)| {
                if k == bucket_key { return false; }
                has_unresolved_definition_ref(v, bucket_key)
            })
        }
        _ => false,
    }
}

fn resolve_node(node: Value, root: &Map<String, Value>, visited: &mut HashSet<String>) -> Value {
    match node {
        Value::Array(arr) => Value::Array(arr.into_iter().map(|item| resolve_node(item, root, visited)).collect()),
        Value::Object(mut obj) => {
            if let Some(Value::String(ref_key)) = obj.get("$ref").cloned() {
                if is_local_json_pointer_ref(&ref_key) {
                    if visited.contains(&ref_key) {
                        return Value::Object(obj);
                    }
                    if let Some(resolved) = resolve_local_json_pointer(root, &ref_key) {
                        visited.insert(ref_key.clone());
                        let resolved = resolve_node(resolved, root, visited);
                        visited.remove(&ref_key);
                        if let Value::Object(mut resolved_obj) = resolved {
                            for (k, v) in obj {
                                if k == "$ref" { continue; }
                                resolved_obj.insert(k, resolve_node(v, root, visited));
                            }
                            return Value::Object(resolved_obj);
                        }
                        return resolved;
                    }
                }
                return Value::Object(obj);
            }
            let resolved: Map<String, Value> = obj
                .into_iter()
                .map(|(k, v)| (k, resolve_node(v, root, visited)))
                .collect();
            Value::Object(resolved)
        }
        other => other,
    }
}

fn is_local_json_pointer_ref(r: &str) -> bool {
    r == "#" || r.starts_with("#/")
}

fn resolve_local_json_pointer(root: &Map<String, Value>, r: &str) -> Option<Value> {
    if r == "#" {
        return Some(Value::Object(root.clone()));
    }
    let mut current: Value = Value::Object(root.clone());
    for raw_part in r[2..].split('/') {
        let part = unescape_json_pointer_part(raw_part);
        current = match current {
            Value::Object(mut obj) => obj.remove(&part)?,
            Value::Array(arr) => {
                let idx = part.parse::<usize>().ok()?;
                arr.into_iter().nth(idx)?
            }
            _ => return None,
        };
    }
    Some(current)
}

fn unescape_json_pointer_part(part: &str) -> String {
    part.replace("~1", "/").replace("~0", "~")
}

fn ensure_kimi_property_types(schema: Map<String, Value>) -> Map<String, Value> {
    let mut normalized = clone_json_value(&Value::Object(schema));
    if let Value::Object(ref mut obj) = normalized {
        recurse_schema(obj);
    }
    match normalized {
        Value::Object(m) => m,
        _ => panic!("JSON Schema root must normalize to an object"),
    }
}

fn recurse_schema(node: &mut Value) {
    if let Value::Object(obj) = node {
        visit_child_schemas(obj, normalize_property);
    }
}

fn visit_child_schemas(node: &mut Map<String, Value>, visit: fn(&mut Value)) {
    for slot in CHILD_SCHEMA_SLOTS {
        let Some(value) = node.get_mut(slot.key) else { continue };
        match slot.kind {
            SchemaSlotKind::Single => {
                if value.is_object() { visit(value); }
            }
            SchemaSlotKind::Array => {
                if let Value::Array(arr) = value {
                    for item in arr { visit(item); }
                }
            }
            SchemaSlotKind::Map => {
                if let Value::Object(obj) = value {
                    for (_, item) in obj { visit(item); }
                }
            }
            SchemaSlotKind::SchemaOrArray => {
                if value.is_object() {
                    visit(value);
                } else if let Value::Array(arr) = value {
                    for item in arr { visit(item); }
                }
            }
        }
    }
}

fn normalize_property(node: &mut Value) {
    let Some(obj) = node.as_object_mut() else { return };
    if !obj.contains_key("type") && !has_any_key(obj, TYPE_COMPLETION_SKIP_KEYS) {
        let inferred = if let Some(Value::Array(values)) = obj.get("enum") {
            if !values.is_empty() {
                infer_type_from_values(values)
            } else {
                infer_type_from_structure(obj)
            }
        } else if obj.contains_key("const") {
            if let Some(v) = obj.get("const") {
                infer_type_from_values(&[v.clone()])
            } else {
                infer_type_from_structure(obj)
            }
        } else {
            infer_type_from_structure(obj)
        };
        obj.insert("type".into(), Value::String(inferred));
    }
    recurse_schema(node);
}

fn has_any_key(obj: &Map<String, Value>, keys: &[&str]) -> bool {
    keys.iter().any(|k| obj.contains_key(*k))
}

fn infer_type_from_structure(obj: &Map<String, Value>) -> String {
    if has_any_key(obj, &object_structure_keys().into_iter().collect::<Vec<_>>()) {
        return "object".into();
    }
    if has_any_key(obj, &array_structure_keys().into_iter().collect::<Vec<_>>()) {
        return "array".into();
    }
    if has_any_key(obj, &string_structure_keys().into_iter().collect::<Vec<_>>()) {
        return "string".into();
    }
    if has_any_key(obj, &numeric_structure_keys().into_iter().collect::<Vec<_>>()) {
        return "number".into();
    }
    "string".into()
}

fn infer_type_from_values(values: &[Value]) -> String {
    let mut inferred: HashSet<String> = HashSet::new();
    for v in values {
        if let Some(t) = infer_value_type(v) {
            inferred.insert(t);
        } else {
            panic!("Cannot infer JSON Schema type from non-JSON enum or const value.");
        }
    }
    let normalized = normalize_inferred_types(inferred);
    if normalized.len() == 1 {
        normalized.into_iter().next().unwrap()
    } else {
        panic!("Mixed JSON Schema enum or const types are not supported by Kimi tool schemas.");
    }
}

fn infer_value_type(value: &Value) -> Option<String> {
    match value {
        Value::Null => Some("null".into()),
        Value::Array(_) => Some("array".into()),
        Value::String(_) => Some("string".into()),
        Value::Number(n) => Some(if n.is_i64() || n.is_u64() { "integer".into() } else { "number".into() }),
        Value::Bool(_) => Some("boolean".into()),
        Value::Object(_) => Some("object".into()),
    }
}

fn normalize_inferred_types(types: HashSet<String>) -> Vec<String> {
    let mut normalized = types;
    if normalized.contains("number") {
        normalized.remove("integer");
    }
    let order = ["string", "number", "integer", "boolean", "object", "array", "null"];
    order.iter().filter(|t| normalized.contains(**t)).map(|t| t.to_string()).collect()
}

fn clone_json_value(value: &Value) -> Value {
    match value {
        Value::Array(arr) => Value::Array(arr.iter().map(clone_json_value).collect()),
        Value::Object(obj) => Value::Object(obj.iter().map(|(k, v)| (k.clone(), clone_json_value(v))).collect()),
        other => other.clone(),
    }
}
```

- [ ] 在 `lib.rs` 加入模块导出：

```rust
pub mod kimi_schema;
pub use kimi_schema::normalize_kimi_tool_schema;
```

- [ ] 运行测试：

```bash
cd rust-ody && cargo test -p kosong-rs kimi_schema::tests
```

预期：全部通过。

- [ ] 运行 crate 级编译：

```bash
cd rust-ody && cargo check -p kosong-rs
```

- [ ] Commit: `feat(kosong-rs): port kimi-schema tool parameter normalizer`

---

## Part 1 Self-Review

- [ ] 1. Spec-coverage table:
  | 路线图条目 | Task | 状态 |
  |---|---|---|
  | 4.2.5 capability registry（Kimi/DeepSeek/GLM） | Task 1 | covered |
  | Kimi `choices[0].usage` 解析支持 | Task 2 | covered |
  | Kimi 工具参数归一化 | Task 3 | covered |
- [ ] 2. Placeholder scan: 无 TODO/TBD；所有函数均给出完整实现。
- [ ] 3. No phantom tasks: 每个 Task 均有代码、命令、预期、commit。
- [ ] 4. Dependency soundness: Task 1/2/3 彼此无真正运行时依赖，均在 Phase 1 内；Task 2 不改变既有函数签名。
- [ ] 5. Caller & build soundness: Task 2 新增 `parse_stream_response_with_usage_extractor`，保留 `parse_stream_response` 原签名；Task 3 新增模块，不影响既有签名。`cargo check -p kosong-rs` 验证通过。
- [ ] 6. Test-the-risk: Task 1 测试 capability 前缀匹配边界；Task 2 测试 usage 位置优先级；Task 3 测试类型推断、`$ref` 展开与循环引用保留。
- [ ] 7. Type consistency: `ModelCapability`、`TokenUsage`、`StreamedMessagePart` 复用既有定义；新增函数签名与既有模块一致。

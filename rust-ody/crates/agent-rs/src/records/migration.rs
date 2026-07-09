use serde_json::{Map, Value as JsonValue};

/// Current agent wire protocol version.
pub const AGENT_WIRE_PROTOCOL_VERSION: &str = "1.3";

/// A single JSON object on the wire before or during migration.
pub type WireMigrationRecord = Map<String, JsonValue>;

/// One step in the wire migration chain.
pub struct WireMigration {
    pub source_version: &'static str,
    pub target_version: &'static str,
    pub migrate_record: fn(&WireMigrationRecord) -> WireMigrationRecord,
}

const MIGRATIONS: &[WireMigration] = &[
    WireMigration {
        source_version: "1.0",
        target_version: "1.1",
        migrate_record: migrate_v1_0_to_v1_1_record,
    },
    WireMigration {
        source_version: "1.1",
        target_version: "1.2",
        migrate_record: migrate_v1_1_to_v1_2_record,
    },
    WireMigration {
        source_version: "1.2",
        target_version: "1.3",
        migrate_record: migrate_v1_2_to_v1_3_record,
    },
];

/// Return true if `read_version` is newer than the current protocol version.
pub fn is_newer_wire_version(read_version: &str) -> bool {
    compare_wire_versions(read_version, AGENT_WIRE_PROTOCOL_VERSION) > 0
}

/// Return the sequence of migrations needed to bring `read_version` up to date.
pub fn resolve_wire_migrations(read_version: &str) -> Vec<&'static WireMigration> {
    if compare_wire_versions(read_version, AGENT_WIRE_PROTOCOL_VERSION) >= 0 {
        return Vec::new();
    }

    let mut migrations = Vec::new();
    let mut version = read_version;
    while compare_wire_versions(version, AGENT_WIRE_PROTOCOL_VERSION) < 0 {
        let migration = find_migration(version)
            .unwrap_or_else(|| panic!("Missing wire migration for version {}", version));
        version = migration.target_version;
        migrations.push(migration);
    }
    migrations
}

/// Apply a list of migrations to a single record.
pub fn migrate_wire_record(
    record: &WireMigrationRecord,
    migrations: &[&WireMigration],
) -> WireMigrationRecord {
    migrations
        .iter()
        .fold(record.clone(), |current, migration| {
            (migration.migrate_record)(&current)
        })
}

/// Apply the migration chain to a list of records.
///
/// If `read_version` is `None`, all known migrations are applied (useful when
/// reading records whose version header is missing).
pub fn migrate_wire_records(
    records: &[WireMigrationRecord],
    read_version: Option<&str>,
) -> Vec<WireMigrationRecord> {
    let migrations: Vec<_> = match read_version {
        Some(v) => resolve_wire_migrations(v),
        None => MIGRATIONS.iter().collect(),
    };
    records
        .iter()
        .map(|record| migrate_wire_record(record, &migrations))
        .collect()
}

fn find_migration(version: &str) -> Option<&'static WireMigration> {
    MIGRATIONS.iter().find(|m| m.source_version == version)
}

fn compare_wire_versions(a: &str, b: &str) -> i64 {
    let parts_a: Vec<i64> = a.split('.').filter_map(|s| s.parse().ok()).collect();
    let parts_b: Vec<i64> = b.split('.').filter_map(|s| s.parse().ok()).collect();
    let max_len = parts_a.len().max(parts_b.len());
    for i in 0..max_len {
        let diff = parts_a.get(i).unwrap_or(&0) - parts_b.get(i).unwrap_or(&0);
        if diff != 0 {
            return diff;
        }
    }
    0
}

// ---------------------------------------------------------------------------
// v1.0 -> v1.1
// ---------------------------------------------------------------------------

/// v1.0 tool calls were wrapped in `function: { name, arguments }`.
/// v1.1 flattens those fields onto the tool call object.
fn migrate_v1_0_to_v1_1_record(record: &WireMigrationRecord) -> WireMigrationRecord {
    if record.get("type").and_then(|v| v.as_str()) != Some("context.append_message") {
        return record.clone();
    }

    let mut out = record.clone();
    if let Some(message) = out.get_mut("message").and_then(|v| v.as_object_mut()) {
        if let Some(tool_calls) = message.get_mut("toolCalls").and_then(|v| v.as_array_mut()) {
            for tc in tool_calls.iter_mut() {
                if let Some(obj) = tc.as_object_mut() {
                    if let Some(function) =
                        obj.remove("function").and_then(|v| v.as_object().cloned())
                    {
                        if let Some(name) = function.get("name").cloned() {
                            obj.insert("name".to_string(), name);
                        }
                        if let Some(arguments) = function.get("arguments").cloned() {
                            obj.insert("arguments".to_string(), arguments);
                        }
                    }
                }
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// v1.1 -> v1.2
// ---------------------------------------------------------------------------

fn migrate_v1_1_to_v1_2_record(record: &WireMigrationRecord) -> WireMigrationRecord {
    if record.get("type").and_then(|v| v.as_str()) != Some("permission.record_approval_result") {
        return record.clone();
    }

    let result = match record.get("result").and_then(|v| v.as_object()) {
        Some(r) => r,
        None => return record.clone(),
    };
    if result.get("decision").and_then(|v| v.as_str()) != Some("approved")
        || result.get("scope").and_then(|v| v.as_str()) != Some("session")
    {
        return record.clone();
    }
    if record.contains_key("sessionApprovalRule") {
        return record.clone();
    }

    let action = record.get("action").and_then(|v| v.as_str()).unwrap_or("");
    let tool_name = record
        .get("toolName")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let pattern = match action {
        "run command" => Some("Bash"),
        "stop background task" => Some("TaskStop"),
        "edit file" | "edit file outside of working directory" | "write file" => Some("Write"),
        "run command in plan mode" | "run background command" => None,
        _ => Some(tool_name),
    };

    let Some(pattern) = pattern else {
        return record.clone();
    };

    let mut out = record.clone();
    out.insert(
        "sessionApprovalRule".to_string(),
        JsonValue::String(pattern.to_string()),
    );
    out
}

// ---------------------------------------------------------------------------
// v1.2 -> v1.3
// ---------------------------------------------------------------------------

fn migrate_v1_2_to_v1_3_record(record: &WireMigrationRecord) -> WireMigrationRecord {
    // Blobref URLs are handled transparently at read/write time.
    record.clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compare_versions_orders_correctly() {
        assert_eq!(compare_wire_versions("1.0", "1.1"), -1);
        assert_eq!(compare_wire_versions("1.3", "1.3"), 0);
        assert_eq!(compare_wire_versions("1.10", "1.3"), 7);
        assert_eq!(compare_wire_versions("2.0", "1.3"), 1);
    }

    #[test]
    fn resolves_no_migrations_for_current_version() {
        assert!(resolve_wire_migrations("1.3").is_empty());
        assert!(resolve_wire_migrations("1.4").is_empty());
    }

    #[test]
    fn resolves_chain_from_1_0() {
        let migrations = resolve_wire_migrations("1.0");
        assert_eq!(migrations.len(), 3);
        assert_eq!(migrations[0].target_version, "1.1");
        assert_eq!(migrations[1].target_version, "1.2");
        assert_eq!(migrations[2].target_version, "1.3");
    }

    #[test]
    fn migrates_tool_call_function_wrapper() {
        let record = serde_json::json!({
            "type": "context.append_message",
            "time": 1,
            "message": {
                "role": "assistant",
                "content": [],
                "toolCalls": [
                    {
                        "type": "function",
                        "id": "call_1",
                        "function": { "name": "read", "arguments": "{\"path\":\"a\"}" }
                    }
                ]
            }
        });
        let record = record.as_object().unwrap().clone();
        let migrated = migrate_v1_0_to_v1_1_record(&record);
        let tool_calls = migrated["message"]["toolCalls"].as_array().unwrap();
        assert_eq!(tool_calls[0]["name"], "read");
        assert_eq!(tool_calls[0]["arguments"], "{\"path\":\"a\"}");
        assert!(!tool_calls[0].as_object().unwrap().contains_key("function"));
    }

    #[test]
    fn leaves_unrelated_records_unchanged() {
        let record = serde_json::json!({ "type": "metadata", "protocol_version": "1.0" });
        let record = record.as_object().unwrap().clone();
        assert_eq!(migrate_v1_0_to_v1_1_record(&record), record);
    }

    #[test]
    fn migrates_session_approval_rule() {
        let record = serde_json::json!({
            "type": "permission.record_approval_result",
            "turnId": 1,
            "toolCallId": "tc1",
            "toolName": "Bash",
            "action": "run command",
            "result": { "decision": "approved", "scope": "session" }
        });
        let record = record.as_object().unwrap().clone();
        let migrated = migrate_v1_1_to_v1_2_record(&record);
        assert_eq!(migrated["sessionApprovalRule"], "Bash");
    }

    #[test]
    fn skips_unrestorable_session_approval_action() {
        let record = serde_json::json!({
            "type": "permission.record_approval_result",
            "turnId": 1,
            "toolCallId": "tc1",
            "toolName": "Bash",
            "action": "run background command",
            "result": { "decision": "approved", "scope": "session" }
        });
        let record = record.as_object().unwrap().clone();
        let migrated = migrate_v1_1_to_v1_2_record(&record);
        assert!(!migrated.contains_key("sessionApprovalRule"));
    }

    #[test]
    fn skips_when_session_approval_rule_already_present() {
        let record = serde_json::json!({
            "type": "permission.record_approval_result",
            "turnId": 1,
            "toolCallId": "tc1",
            "toolName": "Bash",
            "action": "run command",
            "sessionApprovalRule": "Custom",
            "result": { "decision": "approved", "scope": "session" }
        });
        let record = record.as_object().unwrap().clone();
        let migrated = migrate_v1_1_to_v1_2_record(&record);
        assert_eq!(migrated["sessionApprovalRule"], "Custom");
    }

    #[test]
    fn applies_full_chain_from_1_0() {
        let records = vec![serde_json::json!({
            "type": "context.append_message",
            "message": {
                "role": "assistant",
                "content": [],
                "toolCalls": [
                    {
                        "type": "function",
                        "id": "call_1",
                        "function": { "name": "read", "arguments": "{}" }
                    }
                ]
            }
        })];
        let records: Vec<_> = records
            .into_iter()
            .map(|v| v.as_object().unwrap().clone())
            .collect();
        let migrated = migrate_wire_records(&records, Some("1.0"));
        assert_eq!(migrated[0]["message"]["toolCalls"][0]["name"], "read");
    }
}

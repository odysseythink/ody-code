use agent_rs::context::notification_xml::render_notification_xml;
use serde_json::{json, Map, Value};

fn data_from_json(value: Value) -> Map<String, Value> {
    value.as_object().unwrap().clone()
}

#[test]
fn renders_task_notification_with_escaped_attributes_and_bounded_tail() {
    // 21 lines so that only the last 20 are retained.
    let tail_output = (1..=21)
        .map(|i| format!("line{i}"))
        .collect::<Vec<_>>()
        .join("\n");
    let data = data_from_json(json!({
      "id": "task-1",
      "category": "task",
      "type": "terminated",
      "source_kind": "background_task",
      "source_id": "bg-1",
      "title": "Task done",
      "severity": "info",
      "body": "Body line",
      "tail_output": tail_output
    }));
    let xml = render_notification_xml(&data);
    assert!(xml.starts_with("<notification id=\"task-1\" category=\"task\" type=\"terminated\" source_kind=\"background_task\" source_id=\"bg-1\">"));
    assert!(xml.contains("Title: Task done"));
    assert!(xml.contains("Severity: info"));
    assert!(xml.contains("Body line"));
    assert!(xml.contains("<task-notification>"));
    // tail is last 20 lines and <= 3000 chars
    assert!(!xml.contains("line1\n"));
    assert!(xml.contains("line21"));
    assert!(xml.ends_with("</notification>"));
}

#[test]
fn escapes_attribute_values() {
    let data = data_from_json(json!({
      "id": "a&b\"c",
      "category": "x",
      "type": "y",
      "source_kind": "z",
      "source_id": "w"
    }));
    let xml = render_notification_xml(&data);
    assert!(xml.starts_with("<notification id=\"a&amp;b&quot;c\""));
}

#[test]
fn renders_agent_id_attribute_when_present() {
    let data = data_from_json(json!({
      "id": "n",
      "category": "c",
      "type": "t",
      "source_kind": "background_task",
      "source_id": "s",
      "agent_id": "agent-42"
    }));
    let xml = render_notification_xml(&data);
    assert!(xml.contains(" agent_id=\"agent-42\""));
}

#[test]
fn omits_agent_id_attribute_when_absent() {
    let data = data_from_json(json!({
      "id": "n",
      "category": "c",
      "type": "t",
      "source_kind": "background_task",
      "source_id": "s"
    }));
    let xml = render_notification_xml(&data);
    assert!(!xml.contains("agent_id"));
}

#[test]
fn omits_empty_title_severity_body_lines() {
    let data = data_from_json(json!({
      "id": "n",
      "category": "c",
      "type": "t",
      "source_kind": "cron_job",
      "source_id": "s"
    }));
    let xml = render_notification_xml(&data);
    assert!(!xml.contains("Title:"));
    assert!(!xml.contains("Severity:"));
    assert!(!xml.contains("<task-notification>"));
}

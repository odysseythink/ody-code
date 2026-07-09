use serde_json::Value;

mod harvest_ody_markers;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DebtLedgerMarker {
    pub file: String,
    pub line: u32,
    pub ceiling: String,
    pub upgrade: String,
    pub rot: bool,
}

/// Parses a single raw grep output line such as
/// `src/foo.rs:12: // ody: hardcoded timeout, use config`
/// or `src/foo.rs:12:# ody: hardcoded timeout, use config`.
pub fn parse_ody_marker(raw_line: &str) -> Option<DebtLedgerMarker> {
    let re = regex::Regex::new(r"^(.+?):(\d+):\s*(?://|#)\s*ody:\s*(.*)$").ok()?;
    let caps = re.captures(raw_line.trim())?;
    let file = caps.get(1)?.as_str().to_string();
    let line: u32 = caps.get(2)?.as_str().parse().ok()?;
    let body = caps.get(3)?.as_str().trim();

    let comma_idx = body.find(',');
    let (ceiling, upgrade, rot) = match comma_idx {
        Some(idx) => {
            let c = body[..idx].trim().to_string();
            let u = body[idx + 1..].trim().to_string();
            let r = u.is_empty();
            (c, u, r)
        }
        None => (body.to_string(), String::new(), true),
    };

    Some(DebtLedgerMarker {
        file,
        line,
        ceiling,
        upgrade,
        rot,
    })
}

/// Renders a list of markers as a Chinese-first markdown ledger.
pub fn render_debt_ledger(markers: &[DebtLedgerMarker], truncated: bool) -> String {
    if markers.is_empty() {
        return "未找到 `ody:` 债务标记。台账干净。".to_string();
    }

    let mut groups: std::collections::BTreeMap<String, Vec<DebtLedgerMarker>> =
        std::collections::BTreeMap::new();
    for m in markers {
        groups.entry(m.file.clone()).or_default().push(m.clone());
    }

    let mut lines: Vec<String> = Vec::new();
    for (file, mut file_markers) in groups {
        lines.push(format!("### {file}"));
        file_markers.sort_by_key(|m| m.line);
        for m in file_markers {
            let rot_tag = if m.rot { " ⚠️ rot" } else { "" };
            let upgrade_display = if m.upgrade.is_empty() {
                "（未指定）".to_string()
            } else {
                m.upgrade.clone()
            };
            lines.push(format!(
                "{}:{} — {}。天花板：{}。升级：{}{}",
                m.file, m.line, m.ceiling, m.ceiling, upgrade_display, rot_tag
            ));
        }
        lines.push(String::new());
    }

    let total_rot = markers.iter().filter(|m| m.rot).count();
    lines.push(format!(
        "**汇总**：{} 个标记，{} 个 rot 风险。",
        markers.len(),
        total_rot
    ));
    if truncated {
        lines.push("结果已截断至前 200 条；如需完整扫描，请指定更小的目录或文件。".to_string());
    }
    lines.join("\n")
}

pub use harvest_ody_markers::HarvestOdyMarkersTool;

pub trait TelemetryClient: Send + Sync {
    fn track(&self, event: &str, properties: Value);
}

/// No-op telemetry client used when the host does not provide one.
pub struct NoopTelemetryClient;

impl TelemetryClient for NoopTelemetryClient {
    fn track(&self, _event: &str, _properties: Value) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_double_slash_marker() {
        let line = "src/foo.rs:12:  // ody: hardcoded timeout, use config";
        let m = parse_ody_marker(line).unwrap();
        assert_eq!(m.file, "src/foo.rs");
        assert_eq!(m.line, 12);
        assert_eq!(m.ceiling, "hardcoded timeout");
        assert_eq!(m.upgrade, "use config");
        assert!(!m.rot);
    }

    #[test]
    fn parses_hash_marker_without_comma() {
        let line = "src/bar.py:7:# ody: missing validation";
        let m = parse_ody_marker(line).unwrap();
        assert_eq!(m.file, "src/bar.py");
        assert_eq!(m.line, 7);
        assert_eq!(m.ceiling, "missing validation");
        assert_eq!(m.upgrade, "");
        assert!(m.rot);
    }

    #[test]
    fn ignores_non_marker_line() {
        assert!(parse_ody_marker("src/foo.rs:1: // TODO: fix me").is_none());
    }

    #[test]
    fn renders_empty_ledger() {
        let out = render_debt_ledger(&[], false);
        assert!(out.contains("未找到"));
    }

    #[test]
    fn renders_grouped_ledger() {
        let markers = vec![
            DebtLedgerMarker {
                file: "a.rs".into(),
                line: 2,
                ceiling: "hardcoded".into(),
                upgrade: "config".into(),
                rot: false,
            },
            DebtLedgerMarker {
                file: "a.rs".into(),
                line: 1,
                ceiling: "missing test".into(),
                upgrade: "".into(),
                rot: true,
            },
        ];
        let out = render_debt_ledger(&markers, true);
        assert!(out.contains("### a.rs"));
        assert!(out.contains("2 个标记"));
        assert!(out.contains("1 个 rot 风险"));
        assert!(out.contains("结果已截断"));
    }
}

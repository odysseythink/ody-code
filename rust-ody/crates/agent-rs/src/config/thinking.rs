pub use kosong_rs::provider::ThinkingEffort;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThinkingConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
}

const DEFAULT_THINKING_EFFORT: ThinkingEffort = ThinkingEffort::High;

pub fn resolve_thinking_effort(
    requested: Option<&str>,
    defaults: Option<&ThinkingConfig>,
) -> ThinkingEffort {
    let config_effort = defaults
        .and_then(|c| c.effort.as_deref())
        .and_then(parse_effort)
        .unwrap_or(DEFAULT_THINKING_EFFORT);

    let normalized = requested.map(|s| s.trim().to_lowercase());
    match normalized.as_deref() {
        None | Some("") => {
            if defaults.and_then(|c| c.mode.as_deref()) == Some("off") {
                ThinkingEffort::Off
            } else {
                config_effort
            }
        }
        Some("off") => ThinkingEffort::Off,
        Some("on") => config_effort,
        Some(other) => parse_effort(other).unwrap_or(config_effort),
    }
}

fn parse_effort(value: &str) -> Option<ThinkingEffort> {
    match value.trim().to_lowercase().as_str() {
        "low" => Some(ThinkingEffort::Low),
        "medium" => Some(ThinkingEffort::Medium),
        "high" => Some(ThinkingEffort::High),
        "xhigh" => Some(ThinkingEffort::Xhigh),
        "max" => Some(ThinkingEffort::Max),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_request_uses_default_effort() {
        let config = ThinkingConfig {
            mode: None,
            effort: Some("medium".into()),
        };
        assert_eq!(
            resolve_thinking_effort(None, Some(&config)),
            ThinkingEffort::Medium
        );
    }

    #[test]
    fn empty_request_with_mode_off_returns_off() {
        let config = ThinkingConfig {
            mode: Some("off".into()),
            effort: Some("high".into()),
        };
        assert_eq!(
            resolve_thinking_effort(None, Some(&config)),
            ThinkingEffort::Off
        );
    }

    #[test]
    fn on_returns_config_effort() {
        let config = ThinkingConfig {
            mode: None,
            effort: Some("low".into()),
        };
        assert_eq!(
            resolve_thinking_effort(Some("on"), Some(&config)),
            ThinkingEffort::Low
        );
    }

    #[test]
    fn explicit_effort_overrides_config() {
        let config = ThinkingConfig {
            mode: None,
            effort: Some("low".into()),
        };
        assert_eq!(
            resolve_thinking_effort(Some("max"), Some(&config)),
            ThinkingEffort::Max
        );
    }

    #[test]
    fn off_overrides_config() {
        let config = ThinkingConfig {
            mode: None,
            effort: Some("max".into()),
        };
        assert_eq!(
            resolve_thinking_effort(Some("off"), Some(&config)),
            ThinkingEffort::Off
        );
    }

    #[test]
    fn unknown_request_falls_back_to_config() {
        assert_eq!(
            resolve_thinking_effort(Some("weird"), None),
            DEFAULT_THINKING_EFFORT
        );
    }
}

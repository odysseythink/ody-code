use globset::Glob;

use super::types::{PermissionRule, PermissionRuleMatch, PermissionRuleMatchStrategy};
use crate::agent_loop::types::RunnableToolExecution;

/// Parsed representation of a permission rule pattern.
/// Format: `ToolName(arg_glob)` or just `ToolName`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedPattern {
    pub tool_name: String,
    pub arg_pattern: Option<String>,
}

/// Parse a permission rule pattern string.
/// `"Read"` → tool_name: "Read"
/// `"Read(/etc/**)"` → tool_name: "Read", arg: "/etc/**"
/// `"*"` → tool_name: "*"
pub fn parse_pattern(pattern: &str) -> Result<ParsedPattern, ParsePatternError> {
    let pattern = pattern.trim();
    // Find the outermost `(` — everything before it is the tool name,
    // everything inside is the arg glob.
    if let Some(open_paren) = pattern.rfind('(') {
        let tool_name = pattern[..open_paren].trim().to_string();
        let remainder = pattern[open_paren + 1..].trim();
        if let Some(close_paren) = remainder.rfind(')') {
            let arg = remainder[..close_paren].trim().to_string();
            if tool_name.is_empty() {
                return Err(ParsePatternError::EmptyToolName);
            }
            Ok(ParsedPattern {
                tool_name,
                arg_pattern: if arg.is_empty() { None } else { Some(arg) },
            })
        } else {
            Err(ParsePatternError::UnmatchedParen)
        }
    } else if pattern.contains(')') {
        Err(ParsePatternError::UnmatchedParen)
    } else {
        Ok(ParsedPattern {
            tool_name: pattern.to_string(),
            arg_pattern: None,
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ParsePatternError {
    #[error("unmatched parenthesis in pattern")]
    UnmatchedParen,
    #[error("empty tool name in pattern")]
    EmptyToolName,
}

/// Test if a tool name matches a glob pattern string.
/// TS uses `picomatch.isMatch(toolName, parsed.toolName)`. We use `globset`
/// which is ripgrep's high-quality glob library. Both support standard glob
/// syntax (`*`, `**`, `?`, `[abc]`).
fn tool_name_matches(pattern: &str, tool_name: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    // globset::Glob is case-sensitive; TS picomatch is case-sensitive too.
    Glob::new(pattern)
        .ok()
        .map(|g| g.compile_matcher().is_match(tool_name))
        .unwrap_or(false)
}

/// Match a permission rule against a tool call.
pub fn match_permission_rule(
    rule: &PermissionRule,
    tool_name: &str,
    execution: &RunnableToolExecution,
) -> Option<PermissionRuleMatch> {
    let parsed = parse_pattern(&rule.pattern).ok()?;

    if parsed.tool_name != "*" && !tool_name_matches(&parsed.tool_name, tool_name) {
        return None;
    }

    if parsed.arg_pattern.is_none() {
        return Some(PermissionRuleMatch {
            rule: rule.clone(),
            strategy: PermissionRuleMatchStrategy::ToolNameOnly,
            has_rule_args: false,
        });
    }

    let arg_pattern = parsed.arg_pattern.as_ref().unwrap();
    // If execution has a matches_rule fn, call it; otherwise no match
    if let Some(matches_fn) = &execution.matches_rule {
        if matches_fn(arg_pattern) {
            return Some(PermissionRuleMatch {
                rule: rule.clone(),
                strategy: PermissionRuleMatchStrategy::MatchesRule,
                has_rule_args: true,
            });
        }
    }

    None
}

use kosong_rs::provider::{ChatProvider, ModelCapability};

pub const MIN_FLOOR: i64 = 1;
pub const DEFAULT_UNKNOWN_OUTPUT_FALLBACK: i64 = 32000;

const CONTEXT_WINDOW_OVERHEAD_TOKENS: i64 = 8192;
const MAX_CONTEXT_COMPLETION_RATIO: f64 = 0.25;

#[derive(Debug, Clone, Default)]
pub struct CompletionBudgetConfig {
    pub hard_cap: Option<i64>,
    pub fallback: Option<i64>,
}

pub fn resolve_completion_budget(
    reserved_context_size: Option<i64>,
) -> Option<CompletionBudgetConfig> {
    if let Some(size) = reserved_context_size {
        if size > 0 {
            return Some(CompletionBudgetConfig {
                hard_cap: None,
                fallback: Some(size),
            });
        }
        return None;
    }
    Some(CompletionBudgetConfig {
        hard_cap: None,
        fallback: Some(DEFAULT_UNKNOWN_OUTPUT_FALLBACK),
    })
}

pub fn compute_completion_budget_cap(
    budget: &CompletionBudgetConfig,
    capability: &ModelCapability,
    input_tokens: Option<i64>,
) -> i64 {
    let max_output = capability.max_output_tokens;
    let max_context = capability.max_context_tokens;

    let mut cap = budget.hard_cap.unwrap_or_else(|| {
        if max_output > 0 {
            max_output
        } else {
            budget.fallback.unwrap_or(DEFAULT_UNKNOWN_OUTPUT_FALLBACK)
        }
    });

    if max_context > 0 {
        if let Some(input) = input_tokens {
            if input > 0 {
                let remaining = max_context - input - CONTEXT_WINDOW_OVERHEAD_TOKENS;
                cap = cap.min(remaining.max(MIN_FLOOR));
            }
        }
        cap = cap.min((max_context as f64 * MAX_CONTEXT_COMPLETION_RATIO).floor() as i64);
    }

    cap.max(MIN_FLOOR)
}

pub fn apply_completion_budget(
    provider: Box<dyn ChatProvider>,
    budget: Option<&CompletionBudgetConfig>,
    capability: &ModelCapability,
    input_tokens: Option<i64>,
) -> Box<dyn ChatProvider> {
    let Some(budget) = budget else {
        return provider;
    };
    let cap = compute_completion_budget_cap(budget, capability, input_tokens);
    let result = provider.with_max_completion_tokens(cap);
    result.unwrap_or(provider)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_uses_reserved_context_size() {
        let cfg = resolve_completion_budget(Some(50_000));
        assert_eq!(cfg.unwrap().fallback, Some(50_000));
    }

    #[test]
    fn cap_is_limited_by_context_window_ratio() {
        let mut cap = ModelCapability::unknown();
        cap.max_context_tokens = 128_000;
        cap.max_output_tokens = 16_000;
        let budget = CompletionBudgetConfig {
            hard_cap: None,
            fallback: Some(32_000),
        };
        let result = compute_completion_budget_cap(&budget, &cap, Some(10_000));
        // cap is limited by both max_output (16_000) and the ratio (32_000) -> min is 16_000
        assert_eq!(result, 16_000);
    }

    #[test]
    fn cap_respects_min_floor() {
        let mut cap = ModelCapability::unknown();
        cap.max_context_tokens = 100;
        let budget = CompletionBudgetConfig {
            hard_cap: None,
            fallback: Some(32_000),
        };
        assert_eq!(
            compute_completion_budget_cap(&budget, &cap, Some(10_000)),
            MIN_FLOOR
        );
    }
}

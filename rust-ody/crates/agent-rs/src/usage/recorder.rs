use std::collections::HashMap;

use kosong_rs::usage::TokenUsage;
use serde::{Deserialize, Serialize};

pub use crate::records::nested::UsageRecordScope;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageStatus {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub by_model: Option<HashMap<String, TokenUsage>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<TokenUsage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_turn: Option<TokenUsage>,
}

fn copy_usage(usage: &TokenUsage) -> TokenUsage {
    TokenUsage {
        input_other: usage.input_other,
        output: usage.output,
        input_cache_read: usage.input_cache_read,
        input_cache_creation: usage.input_cache_creation,
    }
}

fn add_usage(a: TokenUsage, b: TokenUsage) -> TokenUsage {
    TokenUsage {
        input_other: a.input_other + b.input_other,
        output: a.output + b.output,
        input_cache_read: a.input_cache_read + b.input_cache_read,
        input_cache_creation: a.input_cache_creation + b.input_cache_creation,
    }
}

fn total_usage(by_model: &HashMap<String, TokenUsage>) -> Option<TokenUsage> {
    let mut total: Option<TokenUsage> = None;
    for usage in by_model.values() {
        total = Some(match total {
            Some(t) => add_usage(t, copy_usage(usage)),
            None => copy_usage(usage),
        });
    }
    total
}

/// Minimal Agent surface required by `UsageRecorder`.
pub trait UsageRecorderContext {
    fn log_record(&mut self, record: crate::records::AgentRecord);
    fn emit_status_updated(&mut self);
}

pub struct UsageRecorder<C: UsageRecorderContext> {
    context: C,
    by_model: HashMap<String, TokenUsage>,
    current_turn: Option<TokenUsage>,
}

impl<C: UsageRecorderContext> UsageRecorder<C> {
    pub fn new(context: C) -> Self {
        Self {
            context,
            by_model: HashMap::new(),
            current_turn: None,
        }
    }

    pub fn begin_turn(&mut self) {
        self.current_turn = None;
    }

    pub fn end_turn(&mut self) {
        self.current_turn = None;
    }

    pub fn record(&mut self, model: &str, usage: TokenUsage, scope: UsageRecordScope) {
        self.context
            .log_record(crate::records::AgentRecord::UsageRecord {
                time: None,
                model: model.to_owned(),
                usage,
                usage_scope: Some(scope),
            });

        let current = self.by_model.get(model).cloned();
        self.by_model.insert(
            model.to_owned(),
            match current {
                Some(c) => add_usage(c, usage),
                None => copy_usage(&usage),
            },
        );

        if scope == UsageRecordScope::Turn {
            self.current_turn = Some(match self.current_turn {
                Some(c) => add_usage(c, usage),
                None => copy_usage(&usage),
            });
        }

        self.context.emit_status_updated();
    }

    pub fn data(&self) -> UsageStatus {
        let by_model = self.by_model_snapshot();
        let has_by_model = !by_model.is_empty();
        let total = if has_by_model {
            total_usage(&by_model)
        } else {
            None
        };
        UsageStatus {
            by_model: if has_by_model { Some(by_model) } else { None },
            total,
            current_turn: self.current_turn.as_ref().map(copy_usage),
        }
    }

    pub fn status(&self) -> Option<UsageStatus> {
        let status = self.data();
        if status.by_model.is_none() && status.total.is_none() && status.current_turn.is_none() {
            return None;
        }
        Some(status)
    }

    pub fn into_inner(self) -> C {
        self.context
    }

    fn by_model_snapshot(&self) -> HashMap<String, TokenUsage> {
        self.by_model
            .iter()
            .map(|(k, v)| (k.clone(), copy_usage(v)))
            .collect()
    }
}

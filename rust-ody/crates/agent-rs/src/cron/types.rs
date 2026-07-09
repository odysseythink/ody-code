use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CronTaskId(pub String);

impl CronTaskId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

impl fmt::Display for CronTaskId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CronTaskStatus {
    Active,
    Paused,
    Deleted,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CronTask {
    pub id: CronTaskId,
    pub schedule: String,
    pub prompt: String,
    pub status: CronTaskStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_fire_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CronFireContext {
    pub id: CronTaskId,
    pub schedule: String,
    pub prompt: String,
    pub coalesced_count: u64,
    pub fired_at: DateTime<Utc>,
}

pub const CRON_FIRED_EVENT: &str = "cron_fired";

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{bail, Context, Result};
use futures_util::TryStreamExt;
use serde_json::Value as JsonValue;

use super::migration::{migrate_wire_record, resolve_wire_migrations, AGENT_WIRE_PROTOCOL_VERSION};
use super::types::{AgentRecord, AgentRecordPersistence, RawRecordStream};

/// Context set while replaying records so that downstream observers can
/// distinguish restore traffic from live traffic.
#[derive(Debug, Clone, Copy)]
pub struct RestoringContext {
    pub time: Option<i64>,
}

/// Result of [`AgentRecords::replay`].
#[derive(Debug, Clone, Default)]
pub struct ReplayResult {
    /// Human-readable warning if the session was written by a newer protocol.
    pub warning: Option<String>,
    /// Number of records that were replayed (including metadata).
    pub records_replayed: usize,
}

/// High-level record log. Mirrors the TypeScript `AgentRecords` class.
///
/// `P` is the persistence backend; `R` is a user-supplied restore callback that
/// receives each record during replay.
pub struct AgentRecords<P, R> {
    persistence: Option<P>,
    restoring: Option<RestoringContext>,
    metadata_initialized: bool,
    subscribers: Arc<Mutex<HashMap<usize, Box<dyn Fn(&AgentRecord) + Send + Sync>>>>,
    next_subscriber_id: AtomicUsize,
    restore_handler: R,
    app_version: Option<String>,
}

impl<P, R> AgentRecords<P, R>
where
    P: AgentRecordPersistence,
    R: FnMut(&AgentRecord) + Send,
{
    /// Create a new record log.
    ///
    /// `persistence` may be `None` for purely in-memory recording.
    /// `restore_handler` is called for each record during [`Self::replay`].
    pub fn new(persistence: Option<P>, restore_handler: R, app_version: Option<String>) -> Self {
        Self {
            persistence,
            restoring: None,
            metadata_initialized: false,
            subscribers: Arc::new(Mutex::new(HashMap::new())),
            next_subscriber_id: AtomicUsize::new(1),
            restore_handler,
            app_version,
        }
    }

    /// True when a replay is currently in progress.
    pub fn restoring(&self) -> Option<RestoringContext> {
        self.restoring
    }

    /// Subscribe to every record logged outside of replay.
    ///
    /// Returns a closure that removes the subscription when called.
    pub fn subscribe(
        &self,
        handler: impl Fn(&AgentRecord) + Send + Sync + 'static,
    ) -> impl FnOnce() {
        let id = self.next_subscriber_id.fetch_add(1, Ordering::SeqCst);
        {
            let mut subscribers = self.subscribers.lock().unwrap();
            subscribers.insert(id, Box::new(handler));
        }
        let subscribers = self.subscribers.clone();
        move || {
            subscribers.lock().unwrap().remove(&id);
        }
    }

    /// Log a record.
    ///
    /// While restoring this is a no-op. If the record has no timestamp the
    /// current time is stamped on it. A metadata record is automatically
    /// emitted first when persistence is present.
    pub fn log_record(&mut self, mut record: AgentRecord) {
        if self.restoring.is_some() {
            return;
        }

        if record.time().is_none() {
            record = record.with_time(now_ms());
        }

        if let Some(ref mut persistence) = self.persistence {
            if !self.metadata_initialized && record.record_type() != "metadata" {
                persistence.append(AgentRecord::Metadata {
                    time: Some(now_ms()),
                    protocol_version: AGENT_WIRE_PROTOCOL_VERSION.into(),
                    created_at: now_ms(),
                    app_version: self.app_version.clone(),
                    resumed: None,
                });
                self.metadata_initialized = true;
            }
            if record.record_type() == "metadata" {
                self.metadata_initialized = true;
            }
            persistence.append(record.clone());
        }

        self.notify_subscribers(&record);
    }

    /// Restore a single record by invoking the configured restore handler.
    ///
    /// Sets [`Self::restoring`] while the handler runs.
    pub fn restore(&mut self, record: &AgentRecord) {
        self.restoring = Some(RestoringContext {
            time: record.time(),
        });
        (self.restore_handler)(record);
        self.restoring = None;
    }

    /// Read all persisted records, migrate them to the current wire protocol,
    /// replay them through the restore handler, and rewrite the file if needed.
    pub async fn replay(&mut self) -> Result<ReplayResult> {
        let raw_records: Vec<JsonValue> = {
            let persistence = self
                .persistence
                .as_ref()
                .context("No persistence provided for AgentRecords")?;
            persistence.read_raw().await?.try_collect().await?
        };

        // The first record must be metadata so we know which migrations to run.
        let first = raw_records
            .first()
            .context("AgentRecords replay expected at least a metadata record")?;
        let first_type = first.get("type").and_then(|v| v.as_str());
        if first_type != Some("metadata") {
            bail!("AgentRecords replay expected metadata as the first record");
        }

        let read_version = first
            .get("protocol_version")
            .and_then(|v| v.as_str())
            .unwrap_or("1.0")
            .to_string();

        let (should_rewrite, warning, migrations) = if super::migration::is_newer_wire_version(
            &read_version,
        ) {
            (
                    false,
                    Some(format!(
                        "Session wire protocol version {} is newer than the current version {}. Records will be replayed without migration.",
                        read_version, AGENT_WIRE_PROTOCOL_VERSION
                    )),
                    Vec::new(),
                )
        } else {
            let migrations = resolve_wire_migrations(&read_version);
            let rewrite = read_version != AGENT_WIRE_PROTOCOL_VERSION;
            (rewrite, None, migrations)
        };

        let mut replayed_records: Vec<AgentRecord> = Vec::new();
        let process = |raw: JsonValue| -> Result<AgentRecord> {
            let map = raw
                .as_object()
                .cloned()
                .context("record was not a JSON object")?;
            let migrated = migrate_wire_record(&map, &migrations);
            let mut migrated: AgentRecord = serde_json::from_value(JsonValue::Object(migrated))?;
            if let AgentRecord::Metadata {
                time,
                created_at,
                app_version,
                resumed,
                ..
            } = migrated
            {
                migrated = AgentRecord::Metadata {
                    time,
                    protocol_version: AGENT_WIRE_PROTOCOL_VERSION.into(),
                    created_at,
                    app_version,
                    resumed,
                };
            }
            Ok(migrated)
        };

        for raw in raw_records {
            replayed_records.push(process(raw)?);
        }

        for record in &replayed_records {
            self.restore(record);
        }

        if should_rewrite {
            let persistence = self.persistence.as_mut().unwrap();
            persistence.rewrite(&replayed_records);
            persistence.flush().await?;
        }

        if let Some(AgentRecord::Metadata { app_version, .. }) = replayed_records.first() {
            if app_version.as_deref() != self.app_version.as_deref() {
                let persistence = self.persistence.as_mut().unwrap();
                persistence.append(AgentRecord::Metadata {
                    time: Some(now_ms()),
                    protocol_version: AGENT_WIRE_PROTOCOL_VERSION.into(),
                    created_at: now_ms(),
                    app_version: self.app_version.clone(),
                    resumed: Some(true),
                });
                persistence.flush().await?;
            }
        }

        Ok(ReplayResult {
            warning,
            records_replayed: replayed_records.len(),
        })
    }

    /// Read raw JSON records from persistence without migrating or restoring.
    pub async fn read_raw(&self) -> Result<RawRecordStream<'_>> {
        let persistence = self
            .persistence
            .as_ref()
            .context("No persistence provided for AgentRecords")?;
        persistence.read_raw().await
    }

    /// Flush pending records to persistence.
    pub async fn flush(&mut self) -> Result<()> {
        if let Some(ref mut persistence) = self.persistence {
            persistence.flush().await?;
        }
        Ok(())
    }

    fn notify_subscribers(&self, record: &AgentRecord) {
        let subscribers = self.subscribers.lock().unwrap();
        for handler in subscribers.values() {
            handler(record);
        }
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::records::persistence::InMemoryAgentRecordPersistence;
    use futures_util::{StreamExt, TryStreamExt};

    fn test_app_version() -> Option<String> {
        Some("0.0.0".into())
    }

    #[tokio::test]
    async fn log_injects_metadata_before_first_record() {
        let mut records = AgentRecords::new(
            Some(InMemoryAgentRecordPersistence::default()),
            |_| {},
            test_app_version(),
        );
        records.log_record(AgentRecord::ContextClear { time: None });
        records.flush().await.unwrap();

        let persistence = records.persistence.as_ref().unwrap();
        let all: Vec<AgentRecord> = persistence
            .read()
            .await
            .unwrap()
            .try_collect()
            .await
            .unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].record_type(), "metadata");
        assert_eq!(all[1].record_type(), "context.clear");
    }

    #[tokio::test]
    async fn replay_invokes_restore_handler() {
        let mut persistence = InMemoryAgentRecordPersistence::default();
        persistence.append(AgentRecord::Metadata {
            time: Some(1),
            protocol_version: "1.3".into(),
            created_at: 1,
            app_version: None,
            resumed: None,
        });
        persistence.append(AgentRecord::ContextClear { time: Some(2) });

        let restored = Arc::new(Mutex::new(Vec::new()));
        let restored_clone = restored.clone();
        let mut records = AgentRecords::new(
            Some(persistence),
            move |r| {
                restored_clone
                    .lock()
                    .unwrap()
                    .push(r.record_type().to_string())
            },
            test_app_version(),
        );

        let result = records.replay().await.unwrap();
        assert_eq!(result.records_replayed, 2);
        let seen = restored.lock().unwrap();
        assert_eq!(seen[..], ["metadata", "context.clear"]);
    }

    #[tokio::test]
    async fn replay_fails_without_leading_metadata() {
        let mut persistence = InMemoryAgentRecordPersistence::default();
        persistence.append(AgentRecord::ContextClear { time: Some(1) });

        let mut records = AgentRecords::new(Some(persistence), |_| {}, test_app_version());
        assert!(records.replay().await.is_err());
    }

    #[test]
    fn subscriber_receives_logged_records() {
        let mut records = AgentRecords::new(
            Some(InMemoryAgentRecordPersistence::default()),
            |_| {},
            test_app_version(),
        );
        let received = Arc::new(Mutex::new(Vec::new()));
        let received_clone = received.clone();
        let _unsubscribe = records.subscribe(move |r| {
            received_clone
                .lock()
                .unwrap()
                .push(r.record_type().to_string());
        });

        records.log_record(AgentRecord::ContextClear { time: Some(1) });
        let seen = received.lock().unwrap();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0], "context.clear");
    }

    #[tokio::test]
    async fn raw_read_returns_untyped_json() {
        let mut records = AgentRecords::new(
            Some(InMemoryAgentRecordPersistence::default()),
            |_| {},
            test_app_version(),
        );
        records.log_record(AgentRecord::ContextClear { time: Some(5) });
        records.flush().await.unwrap();

        let mut stream = records.read_raw().await.unwrap();
        let mut types = Vec::new();
        while let Some(result) = stream.next().await {
            let raw = result.unwrap();
            types.push(raw["type"].as_str().unwrap().to_string());
        }
        assert_eq!(types, &["metadata", "context.clear"]);
    }

    #[tokio::test]
    async fn replay_migrates_old_tool_call_wrapper() {
        use serde_json::json;

        let mut persistence = InMemoryAgentRecordPersistence::default();
        persistence.append(AgentRecord::Metadata {
            time: Some(1),
            protocol_version: "1.0".into(),
            created_at: 1,
            app_version: Some("0.0.0".into()),
            resumed: None,
        });
        // Append a raw v1.0 record directly so the typed parser accepts it as
        // generic JSON; then replay migrates it.
        persistence.append_raw(json!({
            "type": "context.append_message",
            "time": 2,
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
        }));

        let restored = Arc::new(Mutex::new(Vec::new()));
        let restored_clone = restored.clone();
        let mut records = AgentRecords::new(
            Some(persistence),
            move |r| {
                let json = serde_json::to_value(r).unwrap();
                restored_clone.lock().unwrap().push(
                    json["message"]["toolCalls"][0]["name"]
                        .as_str()
                        .map(String::from),
                );
            },
            test_app_version(),
        );

        let result = records.replay().await.unwrap();
        assert!(result.warning.is_none());
        let seen = restored.lock().unwrap();
        assert_eq!(seen[1], Some("read".to_string()));
    }
}

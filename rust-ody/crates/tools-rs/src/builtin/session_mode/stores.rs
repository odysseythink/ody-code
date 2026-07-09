use crate::builtin::session_mode::{
    BuilderProfileEntry, GameDesignProfileEntry, GameDesignStateStore, LearningEntry,
    OfficeHoursStateStore,
};
use std::sync::Mutex;

pub struct InMemoryOfficeHoursStateStore {
    profiles: Mutex<Vec<BuilderProfileEntry>>,
    learnings: Mutex<Vec<LearningEntry>>,
}

impl InMemoryOfficeHoursStateStore {
    pub fn new() -> Self {
        Self {
            profiles: Mutex::new(Vec::new()),
            learnings: Mutex::new(Vec::new()),
        }
    }

    pub fn profiles(&self) -> Vec<BuilderProfileEntry> {
        self.profiles.lock().unwrap().clone()
    }

    pub fn learnings(&self) -> Vec<LearningEntry> {
        self.learnings.lock().unwrap().clone()
    }
}

#[async_trait::async_trait]
impl OfficeHoursStateStore for InMemoryOfficeHoursStateStore {
    async fn append_profile(&self, entry: BuilderProfileEntry) -> anyhow::Result<()> {
        self.profiles.lock().unwrap().push(entry);
        Ok(())
    }

    async fn append_learning(&self, entry: LearningEntry) -> anyhow::Result<()> {
        self.learnings.lock().unwrap().push(entry);
        Ok(())
    }

    async fn search_learnings(
        &self,
        limit: usize,
        _cross_project: bool,
    ) -> anyhow::Result<Vec<LearningEntry>> {
        let items = self.learnings.lock().unwrap();
        Ok(items.iter().rev().take(limit).cloned().collect())
    }
}

pub struct InMemoryGameDesignStateStore {
    profiles: Mutex<Vec<GameDesignProfileEntry>>,
    learnings: Mutex<Vec<LearningEntry>>,
}

impl InMemoryGameDesignStateStore {
    pub fn new() -> Self {
        Self {
            profiles: Mutex::new(Vec::new()),
            learnings: Mutex::new(Vec::new()),
        }
    }

    pub fn profiles(&self) -> Vec<GameDesignProfileEntry> {
        self.profiles.lock().unwrap().clone()
    }

    pub fn learnings(&self) -> Vec<LearningEntry> {
        self.learnings.lock().unwrap().clone()
    }
}

#[async_trait::async_trait]
impl GameDesignStateStore for InMemoryGameDesignStateStore {
    async fn append_profile(&self, entry: GameDesignProfileEntry) -> anyhow::Result<()> {
        self.profiles.lock().unwrap().push(entry);
        Ok(())
    }

    async fn append_learning(&self, entry: LearningEntry) -> anyhow::Result<()> {
        self.learnings.lock().unwrap().push(entry);
        Ok(())
    }

    async fn search_learnings(
        &self,
        limit: usize,
        _branch: Option<String>,
    ) -> anyhow::Result<Vec<LearningEntry>> {
        let items = self.learnings.lock().unwrap();
        Ok(items.iter().rev().take(limit).cloned().collect())
    }
}

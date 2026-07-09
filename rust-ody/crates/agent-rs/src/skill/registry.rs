use std::collections::HashMap;

use super::types::{is_inline_skill_type, normalize_skill_name, SkillDefinition};

pub trait SkillRegistry: Send + Sync {
    fn get_skill(&self, name: &str) -> Option<&SkillDefinition>;
    fn list_skills(&self) -> Vec<&SkillDefinition>;
    fn list_invocable_skills(&self, session_mode: Option<&str>) -> Vec<&SkillDefinition>;
    fn render_skill_prompt(&self, skill: &SkillDefinition, raw_args: &str) -> String;
}

pub struct InMemorySkillRegistry {
    by_name: HashMap<String, SkillDefinition>,
}

impl InMemorySkillRegistry {
    pub fn new() -> Self {
        Self {
            by_name: HashMap::new(),
        }
    }

    pub fn register(&mut self, skill: SkillDefinition) {
        self.by_name
            .insert(normalize_skill_name(&skill.name), skill);
    }
}

impl Default for InMemorySkillRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl SkillRegistry for InMemorySkillRegistry {
    fn get_skill(&self, name: &str) -> Option<&SkillDefinition> {
        self.by_name.get(&normalize_skill_name(name))
    }

    fn list_skills(&self) -> Vec<&SkillDefinition> {
        let mut skills: Vec<_> = self.by_name.values().collect();
        skills.sort_by(|a, b| a.name.cmp(&b.name));
        skills
    }

    fn list_invocable_skills(&self, session_mode: Option<&str>) -> Vec<&SkillDefinition> {
        self.list_skills()
            .into_iter()
            .filter(|skill| {
                if skill.metadata.disable_model_invocation == Some(true) {
                    return false;
                }
                if !is_inline_skill_type(skill.metadata.skill_type.as_deref()) {
                    return false;
                }
                if let Some(mode) = session_mode {
                    if let Some(hidden) = &skill.metadata.hidden_in_modes {
                        if hidden.iter().any(|m| m.eq_ignore_ascii_case(mode)) {
                            return false;
                        }
                    }
                }
                true
            })
            .collect()
    }

    fn render_skill_prompt(&self, skill: &SkillDefinition, _raw_args: &str) -> String {
        skill.content.clone()
    }
}

use agent_rs::skill::{
    InMemorySkillRegistry, SkillDefinition, SkillMetadata, SkillRegistry, SkillSource,
};

fn sample_skill(name: &str, skill_type: Option<&str>, disabled: bool) -> SkillDefinition {
    SkillDefinition {
        name: name.into(),
        description: "".into(),
        path: format!("/skills/{}.md", name),
        dir: "/skills".into(),
        content: format!("content of {}", name),
        metadata: SkillMetadata {
            skill_type: skill_type.map(|s| s.into()),
            disable_model_invocation: if disabled { Some(true) } else { None },
            hidden_in_modes: None,
            ..SkillMetadata::default()
        },
        source: SkillSource::Project,
        plugin: None,
        mermaid: None,
        d2: None,
    }
}

#[test]
fn registry_get_is_case_insensitive() {
    let mut registry = InMemorySkillRegistry::new();
    registry.register(sample_skill("MySkill", Some("inline"), false));
    assert!(registry.get_skill("mYskill").is_some());
}

#[test]
fn registry_lists_sorted() {
    let mut registry = InMemorySkillRegistry::new();
    registry.register(sample_skill("beta", Some("inline"), false));
    registry.register(sample_skill("alpha", Some("inline"), false));
    let names: Vec<_> = registry
        .list_skills()
        .iter()
        .map(|s| s.name.clone())
        .collect();
    assert_eq!(names, vec!["alpha", "beta"]);
}

#[test]
fn registry_filters_invocable_skills() {
    let mut registry = InMemorySkillRegistry::new();
    registry.register(sample_skill("inline-skill", Some("inline"), false));
    registry.register(sample_skill("knowledge-skill", Some("knowledge"), false));
    registry.register(sample_skill("disabled-skill", Some("inline"), true));
    let invocable = registry.list_invocable_skills(None);
    assert_eq!(invocable.len(), 1);
    assert_eq!(invocable[0].name, "inline-skill");
}

#[test]
fn registry_hides_skills_in_session_mode() {
    let mut registry = InMemorySkillRegistry::new();
    let mut skill = sample_skill("hidden", Some("inline"), false);
    skill.metadata.hidden_in_modes = Some(vec!["design".into()]);
    registry.register(skill);
    assert_eq!(registry.list_invocable_skills(None).len(), 1);
    assert_eq!(registry.list_invocable_skills(Some("design")).len(), 0);
}

#[test]
fn render_skill_prompt_returns_content() {
    let registry = InMemorySkillRegistry::new();
    let skill = sample_skill("x", Some("inline"), false);
    assert_eq!(registry.render_skill_prompt(&skill, "args"), "content of x");
}

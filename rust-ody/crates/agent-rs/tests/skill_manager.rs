use std::collections::HashMap;
use std::sync::Mutex;

use agent_rs::records::nested::PromptOrigin;
use agent_rs::skill::{
    ActivateSkillPayload, InMemorySkillRegistry, SkillActivatedEvent, SkillActivationContext,
    SkillDefinition, SkillManager, SkillMetadata, SkillPromptError, SkillSource,
};
use kosong_rs::message::ContentPart;

#[derive(Debug, Default)]
struct MockCtx {
    events: Mutex<Vec<SkillActivatedEvent>>,
    tracks: Mutex<Vec<(String, HashMap<String, String>)>>,
    prompts: Mutex<Vec<(Vec<ContentPart>, PromptOrigin)>>,
}

impl SkillActivationContext for MockCtx {
    fn emit_skill_activated(&mut self, event: SkillActivatedEvent) {
        self.events.lock().unwrap().push(event);
    }

    fn telemetry_track(&mut self, event_name: &str, properties: HashMap<String, String>) {
        self.tracks
            .lock()
            .unwrap()
            .push((event_name.to_string(), properties));
    }

    fn prompt(
        &mut self,
        input: Vec<ContentPart>,
        origin: PromptOrigin,
    ) -> Result<(), SkillPromptError> {
        self.prompts.lock().unwrap().push((input, origin));
        Ok(())
    }

    fn new_activation_id(&self) -> String {
        "activation-1".to_string()
    }
}

fn sample_skill(name: &str, skill_type: Option<&str>) -> SkillDefinition {
    SkillDefinition {
        name: name.into(),
        description: "".into(),
        path: format!("/skills/{}.md", name),
        dir: "/skills".into(),
        content: format!("content of {}", name),
        metadata: SkillMetadata {
            skill_type: skill_type.map(|s| s.into()),
            ..SkillMetadata::default()
        },
        source: SkillSource::Builtin,
        plugin: None,
        mermaid: None,
        d2: None,
    }
}

#[test]
fn activate_inline_skill_emits_event_and_prompts() {
    let mut registry = InMemorySkillRegistry::new();
    registry.register(sample_skill("my-skill", Some("inline")));

    let ctx = MockCtx::default();
    let mut manager = SkillManager::new(ctx, registry);
    manager
        .activate(ActivateSkillPayload {
            name: "my-skill".into(),
            args: Some("foo=bar".into()),
        })
        .unwrap();

    let ctx = manager.into_inner().0;
    assert_eq!(ctx.events.lock().unwrap().len(), 1);
    assert_eq!(ctx.tracks.lock().unwrap().len(), 1);
    assert_eq!(ctx.prompts.lock().unwrap().len(), 1);

    let event = &ctx.events.lock().unwrap()[0];
    assert_eq!(event.event_type, "skill.activated");
    assert_eq!(event.skill_name, "my-skill");
    assert_eq!(event.skill_args, Some("foo=bar".into()));
    assert_eq!(event.trigger, "user-slash");

    let (parts, origin) = &ctx.prompts.lock().unwrap()[0];
    assert_eq!(parts.len(), 1);
    match origin {
        PromptOrigin::SkillActivation { skill_name, .. } => {
            assert_eq!(skill_name, "my-skill");
        }
        _ => panic!("expected skill_activation origin"),
    }
}

#[test]
fn activate_flow_skill_tracks_flow_invoked() {
    let mut registry = InMemorySkillRegistry::new();
    registry.register(sample_skill("my-flow", Some("flow")));

    let ctx = MockCtx::default();
    let mut manager = SkillManager::new(ctx, registry);
    manager
        .activate(ActivateSkillPayload {
            name: "my-flow".into(),
            args: None,
        })
        .unwrap();

    let ctx = manager.into_inner().0;
    let tracks = ctx.tracks.lock().unwrap();
    assert!(tracks.iter().any(|(name, _)| name == "skill_invoked"));
    assert!(tracks.iter().any(|(name, props)| {
        name == "flow_invoked" && props.get("flow_name") == Some(&"my-flow".to_string())
    }));
}

#[test]
fn activate_unknown_skill_returns_not_found() {
    let registry = InMemorySkillRegistry::new();
    let ctx = MockCtx::default();
    let mut manager = SkillManager::new(ctx, registry);
    let err = manager
        .activate(ActivateSkillPayload {
            name: "missing".into(),
            args: None,
        })
        .unwrap_err();
    assert!(err.to_string().contains("was not found"));
}

#[test]
fn activate_knowledge_skill_returns_unsupported() {
    let mut registry = InMemorySkillRegistry::new();
    registry.register(sample_skill("my-knowledge", Some("knowledge")));

    let ctx = MockCtx::default();
    let mut manager = SkillManager::new(ctx, registry);
    let err = manager
        .activate(ActivateSkillPayload {
            name: "my-knowledge".into(),
            args: None,
        })
        .unwrap_err();
    assert!(err.to_string().contains("cannot be activated by the user"));
}

#[test]
fn record_activation_without_input_does_not_prompt() {
    let registry = InMemorySkillRegistry::new();
    let ctx = MockCtx::default();
    let mut manager = SkillManager::new(ctx, registry);
    manager.record_activation(
        agent_rs::skill::manager::SkillActivationOrigin {
            activation_id: "a".into(),
            skill_name: "x".into(),
            skill_args: None,
            trigger: "model-tool".into(),
            skill_type: None,
            skill_path: None,
            skill_source: None,
        },
        None,
    );
    let ctx = manager.into_inner().0;
    assert_eq!(ctx.events.lock().unwrap().len(), 1);
    assert_eq!(ctx.prompts.lock().unwrap().len(), 0);
}

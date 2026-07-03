pub trait EnvSource: Send + Sync {
    fn var(&self, name: &str) -> Option<String>;
}

struct StdEnv;
impl EnvSource for StdEnv {
    fn var(&self, name: &str) -> Option<String> {
        std::env::var(name).ok()
    }
}

struct FlagDef {
    id: &'static str,
    env: &'static str,
    default: bool,
}

const DEFINITIONS: &[FlagDef] = &[FlagDef {
    id: "micro-compaction",
    env: "ODY_CODE_EXPERIMENTAL_MICRO_COMPACTION",
    default: false,
}];

pub fn enabled(id: &str) -> bool {
    enabled_with_env(id, &StdEnv)
}

fn enabled_with_env(id: &str, env: &dyn EnvSource) -> bool {
    if parse_flag(env.var("ODY_CODE_EXPERIMENTAL_FLAG")).unwrap_or(false) {
        return true;
    }
    DEFINITIONS
        .iter()
        .find(|d| d.id == id)
        .map(|d| parse_flag(env.var(d.env)).unwrap_or(d.default))
        .unwrap_or(false)
}

fn parse_flag(value: Option<String>) -> Option<bool> {
    value.and_then(|v| match v.to_lowercase().as_str() {
        "true" | "1" => Some(true),
        "false" | "0" => Some(false),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    struct MockEnv {
        vars: HashMap<String, String>,
    }
    impl EnvSource for MockEnv {
        fn var(&self, name: &str) -> Option<String> {
            self.vars.get(name).cloned()
        }
    }

    #[test]
    fn master_switch_enables_all() {
        let env = MockEnv {
            vars: [("ODY_CODE_EXPERIMENTAL_FLAG".into(), "true".into())]
                .into_iter()
                .collect(),
        };
        assert!(enabled_with_env("micro-compaction", &env));
    }

    #[test]
    fn specific_env_overrides_default() {
        let mut env = MockEnv {
            vars: HashMap::new(),
        };
        assert!(!enabled_with_env("micro-compaction", &env));
        env.vars.insert(
            "ODY_CODE_EXPERIMENTAL_MICRO_COMPACTION".into(),
            "true".into(),
        );
        assert!(enabled_with_env("micro-compaction", &env));
        env.vars.insert(
            "ODY_CODE_EXPERIMENTAL_MICRO_COMPACTION".into(),
            "false".into(),
        );
        assert!(!enabled_with_env("micro-compaction", &env));
    }
}

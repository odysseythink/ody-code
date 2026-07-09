use crate::records::nested::SessionModeKind;

/// Map a `SessionModeKind` to the config key used to look up the mode-specific
/// model alias (e.g. `"plan"` → `config.models.plan`, `"design"` →
/// `config.models.design`).
/// Mirrors TS `modeModelKeyForKind`.
pub fn mode_model_key_for_kind(kind: SessionModeKind) -> &'static str {
    match kind {
        SessionModeKind::Plan => "plan",
        SessionModeKind::Design => "design",
        SessionModeKind::OfficeHours => "officeHours",
        SessionModeKind::GameDesign => "gameDesign",
    }
}

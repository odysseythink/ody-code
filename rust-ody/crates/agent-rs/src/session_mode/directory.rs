use crate::records::nested::SessionModeKind;

/// Map a `SessionModeKind` to its output subdirectory under `.ody-code/`.
/// Mirrors TS `getModeOutputSubdirectory`.
pub fn get_mode_output_subdirectory(kind: SessionModeKind) -> &'static str {
    match kind {
        SessionModeKind::Plan => "plans",
        SessionModeKind::Design => "designs",
        SessionModeKind::OfficeHours => "products",
        SessionModeKind::GameDesign => "game-design",
    }
}

/// Build the full mode output directory path: `{project_root}/.ody-code/{subdir}/`.
/// Mirrors TS `resolveModeOutputDir`.
pub fn resolve_mode_output_dir(project_root: &str, kind: SessionModeKind) -> String {
    let subdir = get_mode_output_subdirectory(kind);
    format!("{}/.ody-code/{}", project_root, subdir)
}

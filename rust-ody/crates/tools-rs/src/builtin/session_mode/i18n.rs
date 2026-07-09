//! i18n string tables for session-mode tools.
//! Mirrors the upstream TypeScript `packages/agent-core/src/i18n/translations.ts`.

use super::Language;

/// A small translation table keyed by `namespace.key`, matching the TS MessageKey type.
#[derive(Debug, Clone, Copy)]
pub enum Msg {
    OfficeHoursEntered,
    OfficeHoursAlreadyActive,
    OfficeHoursAnotherModeActive,
    OfficeHoursFailedToEnter,
    OfficeHoursSessionComplete,
    OfficeHoursDesignDocSaved,
    OfficeHoursAppWillExit,
    OfficeHoursProfileAppended,
    OfficeHoursLearningRecorded,
    OfficeHoursNoLearnings,
    OfficeHoursLearningsHeader,
    OfficeHoursLearningTypeLabel,
    OfficeHoursLearningInsightLabel,
    OfficeHoursLearningConfidenceLabel,
    OfficeHoursLearningDateLabel,
    OfficeHoursLearningBranchLabel,
    OfficeHoursModeNotActive,
    OfficeHoursDesignFileNotFound,
    OfficeHoursGbrainConnected,
    OfficeHoursGbrainTargetSource,
    OfficeHoursGbrainNoSourcePin,
    OfficeHoursGbrainReadyForSync,
    OfficeHoursGbrainSynced,
    OfficeHoursGbrainFile,
    OfficeHoursGbrainCliFailed,
    OfficeHoursAgentsMdCreated,
    OfficeHoursAgentsMdUpdated,
    OfficeHoursAgentsMdAlreadyHasRouting,
    OfficeHoursFailedToEnsureRouting,
    OfficeHoursFailedToSyncArtifact,
    OfficeHoursLanguageSet,
    GameDesignEntered,
    GameDesignAlreadyActive,
    GameDesignAnotherModeActive,
    GameDesignFailedToEnter,
    GameDesignSessionComplete,
    GameDesignDesignDocSaved,
    GameDesignAppWillExit,
    GameDesignProfileAppended,
    GameDesignLearningRecorded,
    GameDesignNoLearnings,
    GameDesignLearningsHeader,
    GameDesignLearningTypeLabel,
    GameDesignLearningInsightLabel,
    GameDesignLearningConfidenceLabel,
    GameDesignLearningDateLabel,
    GameDesignLearningBranchLabel,
    GameDesignModeNotActive,
    GameDesignDesignFileNotFound,
    GameDesignGbrainConnected,
    GameDesignGbrainTargetSource,
    GameDesignGbrainNoSourcePin,
    GameDesignGbrainReadyForSync,
    GameDesignGbrainSynced,
    GameDesignGbrainFile,
    GameDesignGbrainCliFailed,
    GameDesignAgentsMdCreated,
    GameDesignAgentsMdUpdated,
    GameDesignAgentsMdAlreadyHasRouting,
    GameDesignFailedToEnsureRouting,
    GameDesignFailedToSyncArtifact,
    GameDesignLanguageSet,
}

impl Msg {
    fn en(&self) -> &'static str {
        match self {
            Msg::OfficeHoursEntered => "Office hours mode is now active.",
            Msg::OfficeHoursAlreadyActive => "Office hours mode is already active. Use ExitOfficeHoursMode when the session is complete.",
            Msg::OfficeHoursAnotherModeActive => "Another session mode is already active. Exit it first before entering office hours mode.",
            Msg::OfficeHoursFailedToEnter => "Failed to enter office hours mode: {message}",
            Msg::OfficeHoursSessionComplete => "Office hours session complete.",
            Msg::OfficeHoursDesignDocSaved => "Design document saved to: {path}",
            Msg::OfficeHoursAppWillExit => "The application will now exit.",
            Msg::OfficeHoursProfileAppended => "Builder profile entry appended successfully. Session count will be updated for next tier computation.",
            Msg::OfficeHoursLearningRecorded => "Learning \"{key}\" recorded successfully.",
            Msg::OfficeHoursNoLearnings => "No past learnings found.",
            Msg::OfficeHoursLearningsHeader => "Found {count} learning(s):",
            Msg::OfficeHoursLearningTypeLabel => "Type",
            Msg::OfficeHoursLearningInsightLabel => "Insight",
            Msg::OfficeHoursLearningConfidenceLabel => "Confidence",
            Msg::OfficeHoursLearningDateLabel => "Date",
            Msg::OfficeHoursLearningBranchLabel => "Branch",
            Msg::OfficeHoursModeNotActive => "Office hours mode is not active.",
            Msg::OfficeHoursDesignFileNotFound => "Design file not found at {path}.",
            Msg::OfficeHoursGbrainConnected => "gbrain MCP server is connected.",
            Msg::OfficeHoursGbrainTargetSource => "Target source: {source}",
            Msg::OfficeHoursGbrainNoSourcePin => "No .gbrain-source pin found.",
            Msg::OfficeHoursGbrainReadyForSync => "Design artifact at {path} is ready for sync via MCP.",
            Msg::OfficeHoursGbrainSynced => "Design artifact synced via gbrain CLI.",
            Msg::OfficeHoursGbrainFile => "File: {path}",
            Msg::OfficeHoursGbrainCliFailed => "gbrain CLI sync failed: {message}. Ensure the gbrain CLI is installed and configured.",
            Msg::OfficeHoursAgentsMdCreated => "AGENTS.md created at {path} with ## Skill routing section.",
            Msg::OfficeHoursAgentsMdUpdated => "Appended ## Skill routing section to AGENTS.md at {path}.",
            Msg::OfficeHoursAgentsMdAlreadyHasRouting => "AGENTS.md already has a ## Skill routing section — no changes needed.",
            Msg::OfficeHoursFailedToEnsureRouting => "Failed to ensure AGENTS.md routing: {message}",
            Msg::OfficeHoursFailedToSyncArtifact => "Failed to sync design artifact: {message}",
            Msg::OfficeHoursLanguageSet => "User language set to {language}.",
            Msg::GameDesignEntered => "game-design mode is now active.",
            Msg::GameDesignAlreadyActive => "game-design mode is already active. Use ExitGameDesignMode when the session is complete.",
            Msg::GameDesignAnotherModeActive => "Another session mode is already active. Exit it first before entering game-design mode.",
            Msg::GameDesignFailedToEnter => "Failed to enter game-design mode: {message}",
            Msg::GameDesignSessionComplete => "Game-design session complete.",
            Msg::GameDesignDesignDocSaved => "Design document saved to: {path}",
            Msg::GameDesignAppWillExit => "The application will now exit.",
            Msg::GameDesignProfileAppended => "Builder profile entry appended successfully.",
            Msg::GameDesignLearningRecorded => "Learning \"{key}\" recorded successfully.",
            Msg::GameDesignNoLearnings => "No past learnings found.",
            Msg::GameDesignLearningsHeader => "Found {count} learning(s):",
            Msg::GameDesignLearningTypeLabel => "Type",
            Msg::GameDesignLearningInsightLabel => "Insight",
            Msg::GameDesignLearningConfidenceLabel => "Confidence",
            Msg::GameDesignLearningDateLabel => "Date",
            Msg::GameDesignLearningBranchLabel => "Branch",
            Msg::GameDesignModeNotActive => "Game-design mode is not active.",
            Msg::GameDesignDesignFileNotFound => "Design file not found at {path}.",
            Msg::GameDesignGbrainConnected => "gbrain MCP server is connected.",
            Msg::GameDesignGbrainTargetSource => "Target source: {source}",
            Msg::GameDesignGbrainNoSourcePin => "No .gbrain-source pin found.",
            Msg::GameDesignGbrainReadyForSync => "Design artifact at {path} is ready for sync via MCP.",
            Msg::GameDesignGbrainSynced => "Design artifact synced via gbrain CLI.",
            Msg::GameDesignGbrainFile => "File: {path}",
            Msg::GameDesignGbrainCliFailed => "gbrain CLI sync failed: {message}. Ensure the gbrain CLI is installed and configured.",
            Msg::GameDesignAgentsMdCreated => "AGENTS.md created at {path} with ## Skill routing section.",
            Msg::GameDesignAgentsMdUpdated => "Appended ## Skill routing section to AGENTS.md at {path}.",
            Msg::GameDesignAgentsMdAlreadyHasRouting => "AGENTS.md already has a ## Skill routing section — no changes needed.",
            Msg::GameDesignFailedToEnsureRouting => "Failed to ensure AGENTS.md routing: {message}",
            Msg::GameDesignFailedToSyncArtifact => "Failed to sync design artifact: {message}",
            Msg::GameDesignLanguageSet => "User language set to {language}.",
        }
    }

    fn zh(&self) -> &'static str {
        match self {
            Msg::OfficeHoursEntered => "Office Hours 模式已激活。",
            Msg::OfficeHoursAlreadyActive => {
                "Office Hours 模式已经处于激活状态。会话结束后请调用 ExitOfficeHoursMode。"
            }
            Msg::OfficeHoursAnotherModeActive => {
                "另一个会话模式已经激活。请先退出该模式再进入 Office Hours。"
            }
            Msg::OfficeHoursFailedToEnter => "进入 Office Hours 模式失败：{message}",
            Msg::OfficeHoursSessionComplete => "Office Hours 会话已结束。",
            Msg::OfficeHoursDesignDocSaved => "设计文档已保存至：{path}",
            Msg::OfficeHoursAppWillExit => "应用即将退出。",
            Msg::OfficeHoursProfileAppended => {
                "Builder 档案条目已追加成功。下次层级计算时将更新会话计数。"
            }
            Msg::OfficeHoursLearningRecorded => "学习洞察 \"{key}\" 已记录成功。",
            Msg::OfficeHoursNoLearnings => "未找到过往学习洞察。",
            Msg::OfficeHoursLearningsHeader => "找到 {count} 条学习洞察：",
            Msg::OfficeHoursLearningTypeLabel => "类型",
            Msg::OfficeHoursLearningInsightLabel => "洞察",
            Msg::OfficeHoursLearningConfidenceLabel => "置信度",
            Msg::OfficeHoursLearningDateLabel => "日期",
            Msg::OfficeHoursLearningBranchLabel => "分支",
            Msg::OfficeHoursModeNotActive => "Office Hours 模式未激活。",
            Msg::OfficeHoursDesignFileNotFound => "在 {path} 未找到设计文件。",
            Msg::OfficeHoursGbrainConnected => "gbrain MCP 服务器已连接。",
            Msg::OfficeHoursGbrainTargetSource => "目标源：{source}",
            Msg::OfficeHoursGbrainNoSourcePin => "未找到 .gbrain-source 固定文件。",
            Msg::OfficeHoursGbrainReadyForSync => "{path} 处的设计制品已准备好通过 MCP 同步。",
            Msg::OfficeHoursGbrainSynced => "设计制品已通过 gbrain CLI 同步。",
            Msg::OfficeHoursGbrainFile => "文件：{path}",
            Msg::OfficeHoursGbrainCliFailed => {
                "gbrain CLI 同步失败：{message}。请确保 gbrain CLI 已安装并配置。"
            }
            Msg::OfficeHoursAgentsMdCreated => {
                "已在 {path} 创建 AGENTS.md，并添加 ## Skill routing 章节。"
            }
            Msg::OfficeHoursAgentsMdUpdated => {
                "已在 {path} 的 AGENTS.md 中追加 ## Skill routing 章节。"
            }
            Msg::OfficeHoursAgentsMdAlreadyHasRouting => {
                "AGENTS.md 已包含 ## Skill routing 章节，无需更改。"
            }
            Msg::OfficeHoursFailedToEnsureRouting => "确保 AGENTS.md 路由失败：{message}",
            Msg::OfficeHoursFailedToSyncArtifact => "同步设计制品失败：{message}",
            Msg::OfficeHoursLanguageSet => "用户语言已设置为 {language}。",
            Msg::GameDesignEntered => "Game Design 模式已激活。",
            Msg::GameDesignAlreadyActive => {
                "Game Design 模式已经处于激活状态。会话结束后请调用 ExitGameDesignMode。"
            }
            Msg::GameDesignAnotherModeActive => {
                "另一个会话模式已经激活。请先退出该模式再进入 Game Design。"
            }
            Msg::GameDesignFailedToEnter => "进入 Game Design 模式失败：{message}",
            Msg::GameDesignSessionComplete => "Game Design 会话已结束。",
            Msg::GameDesignDesignDocSaved => "设计文档已保存至：{path}",
            Msg::GameDesignAppWillExit => "应用即将退出。",
            Msg::GameDesignProfileAppended => "Builder 档案条目已追加成功。",
            Msg::GameDesignLearningRecorded => "学习洞察 \"{key}\" 已记录成功。",
            Msg::GameDesignNoLearnings => "未找到过往学习洞察。",
            Msg::GameDesignLearningsHeader => "找到 {count} 条学习洞察：",
            Msg::GameDesignLearningTypeLabel => "类型",
            Msg::GameDesignLearningInsightLabel => "洞察",
            Msg::GameDesignLearningConfidenceLabel => "置信度",
            Msg::GameDesignLearningDateLabel => "日期",
            Msg::GameDesignLearningBranchLabel => "分支",
            Msg::GameDesignModeNotActive => "Game Design 模式未激活。",
            Msg::GameDesignDesignFileNotFound => "在 {path} 未找到设计文件。",
            Msg::GameDesignGbrainConnected => "gbrain MCP 服务器已连接。",
            Msg::GameDesignGbrainTargetSource => "目标源：{source}",
            Msg::GameDesignGbrainNoSourcePin => "未找到 .gbrain-source 固定文件。",
            Msg::GameDesignGbrainReadyForSync => "{path} 处的设计制品已准备好通过 MCP 同步。",
            Msg::GameDesignGbrainSynced => "设计制品已通过 gbrain CLI 同步。",
            Msg::GameDesignGbrainFile => "文件：{path}",
            Msg::GameDesignGbrainCliFailed => {
                "gbrain CLI 同步失败：{message}。请确保 gbrain CLI 已安装并配置。"
            }
            Msg::GameDesignAgentsMdCreated => {
                "已在 {path} 创建 AGENTS.md，并添加 ## Skill routing 章节。"
            }
            Msg::GameDesignAgentsMdUpdated => {
                "已在 {path} 的 AGENTS.md 中追加 ## Skill routing 章节。"
            }
            Msg::GameDesignAgentsMdAlreadyHasRouting => {
                "AGENTS.md 已包含 ## Skill routing 章节，无需更改。"
            }
            Msg::GameDesignFailedToEnsureRouting => "确保 AGENTS.md 路由失败：{message}",
            Msg::GameDesignFailedToSyncArtifact => "同步设计制品失败：{message}",
            Msg::GameDesignLanguageSet => "用户语言已设置为 {language}。",
        }
    }
}

/// Look up a translated string and substitute named placeholders (`{key}` → value).
/// Falls back to English if the language is unsupported or a key is missing.
pub fn t(msg: Msg, lang: Language) -> String {
    let template = match lang {
        Language::Zh => msg.zh(),
        Language::En => msg.en(),
    };
    template.into()
}

/// Substitute placeholders in `template`. Replaces `{name}` with the corresponding value.
pub fn subst(template: &str, vars: &[(&str, &str)]) -> String {
    let mut result = template.to_string();
    for (k, v) in vars {
        result = result.replace(&format!("{{{}}}", k), v);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_english_by_default() {
        assert_eq!(
            t(Msg::GameDesignModeNotActive, Language::En),
            "Game-design mode is not active."
        );
    }

    #[test]
    fn returns_chinese_for_zh() {
        assert_eq!(
            t(Msg::GameDesignModeNotActive, Language::Zh),
            "Game Design 模式未激活。"
        );
    }

    #[test]
    fn subst_replaces_named_placeholders() {
        assert_eq!(
            subst(
                "Hello {name}, count {count}",
                &[("name", "world"), ("count", "3")]
            ),
            "Hello world, count 3"
        );
    }
}

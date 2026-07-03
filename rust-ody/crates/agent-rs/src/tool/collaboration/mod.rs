pub mod background_registrar;
pub mod question_provider;
pub mod skill_provider;
pub mod subagent_host;

pub use background_registrar::AgentBackgroundRegistrar;
pub use question_provider::{AgentQuestionProvider, QuestionCallback};
pub use skill_provider::AgentSkillProvider;
pub use subagent_host::{AgentSubagentHost, SubagentRunFn};

use std::sync::Arc;

use crate::agent::AgentContext;
use crate::background::manager::BackgroundManager;
use crate::tool::bridge::ToolBridge;
use tools_rs::builtin::collaboration::{
    AskUserQuestionOptions, AskUserQuestionTool, SkillTool, SkillToolOptions,
};
use tools_rs::builtin::session_mode::{
    enter_design_mode::EnterDesignModeTool,
    enter_plan_mode::EnterPlanModeTool,
    exit_design_mode::ExitDesignModeTool,
    exit_plan_mode::ExitPlanModeTool,
    game_design::{
        AppendGameDesignLearningTool, AppendGameDesignProfileTool, EnsureGameDesignRoutingTool,
        EnterGameDesignModeTool, ExitGameDesignModeTool, SearchGameDesignLearningsTool,
        SetGameDesignLanguageTool, SyncGameDesignArtifactTool,
    },
    office_hours::{
        AppendBuilderProfileTool, AppendLearningTool, EnsureClaudeMdRoutingTool,
        EnterOfficeHoursModeTool, ExitOfficeHoursModeTool, SearchLearningsTool,
        SetOfficeHoursLanguageTool, SyncOfficeHoursArtifactTool,
    },
    SessionModeProvider,
};
use tools_rs::builtin::{AgentTool, AgentToolOptions};

pub struct CollaborationToolkit;

impl CollaborationToolkit {
    pub fn build_tools(
        context: AgentContext,
        skill_registry: Option<Arc<dyn crate::skill::registry::SkillRegistry>>,
        question_callback: Option<QuestionCallback>,
        subagent_host: Option<Arc<dyn tools_rs::builtin::collaboration::SubagentHost>>,
        background_manager: Option<Arc<BackgroundManager>>,
        session_mode_provider: Option<Arc<dyn SessionModeProvider>>,
    ) -> Vec<Arc<dyn crate::agent_loop::types::ExecutableTool>> {
        let mut tools: Vec<Arc<dyn crate::agent_loop::types::ExecutableTool>> = Vec::new();

        if let Some(registry) = skill_registry {
            let provider = Arc::new(AgentSkillProvider::new(context.weak(), registry));
            tools.push(Arc::new(ToolBridge::new(Arc::new(SkillTool::new(
                provider,
                SkillToolOptions::default(),
            ))))
                as Arc<dyn crate::agent_loop::types::ExecutableTool>);
        }

        if let Some(callback) = question_callback {
            let provider = Arc::new(AgentQuestionProvider::new(callback));
            let registrar = Arc::new(AgentBackgroundRegistrar::new(background_manager.clone()));
            tools.push(Arc::new(ToolBridge::new(Arc::new(AskUserQuestionTool::new(
                provider,
                registrar,
                AskUserQuestionOptions::default(),
            ))))
                as Arc<dyn crate::agent_loop::types::ExecutableTool>);
        }

        if let Some(host) = subagent_host {
            let registrar = Arc::new(AgentBackgroundRegistrar::new(background_manager));
            tools.push(Arc::new(ToolBridge::new(Arc::new(AgentTool::new(
                host,
                Some(registrar),
                AgentToolOptions::default(),
            ))))
                as Arc<dyn crate::agent_loop::types::ExecutableTool>);
        }

        if let Some(provider) = session_mode_provider {
            tools.push(Arc::new(ToolBridge::new(Arc::new(EnterPlanModeTool::new(
                provider.clone(),
            )))));
            tools.push(Arc::new(ToolBridge::new(Arc::new(ExitPlanModeTool::new(
                provider.clone(),
            )))));
            tools.push(Arc::new(ToolBridge::new(Arc::new(
                EnterDesignModeTool::new(provider.clone()),
            ))));
            tools.push(Arc::new(ToolBridge::new(Arc::new(
                ExitDesignModeTool::new(provider.clone()),
            ))));
            tools.push(Arc::new(ToolBridge::new(Arc::new(
                EnterOfficeHoursModeTool::new(provider.clone()),
            ))));
            tools.push(Arc::new(ToolBridge::new(Arc::new(
                ExitOfficeHoursModeTool::new(provider.clone()),
            ))));
            tools.push(Arc::new(ToolBridge::new(Arc::new(
                AppendBuilderProfileTool::new(provider.clone()),
            ))));
            tools.push(Arc::new(ToolBridge::new(Arc::new(
                AppendLearningTool::new(provider.clone()),
            ))));
            tools.push(Arc::new(ToolBridge::new(Arc::new(
                SearchLearningsTool::new(provider.clone()),
            ))));
            tools.push(Arc::new(ToolBridge::new(Arc::new(
                SetOfficeHoursLanguageTool::new(provider.clone()),
            ))));
            tools.push(Arc::new(ToolBridge::new(Arc::new(
                EnsureClaudeMdRoutingTool::new(provider.clone()),
            ))));
            tools.push(Arc::new(ToolBridge::new(Arc::new(
                SyncOfficeHoursArtifactTool::new(provider.clone()),
            ))));
            tools.push(Arc::new(ToolBridge::new(Arc::new(
                EnterGameDesignModeTool::new(provider.clone()),
            ))));
            tools.push(Arc::new(ToolBridge::new(Arc::new(
                ExitGameDesignModeTool::new(provider.clone()),
            ))));
            tools.push(Arc::new(ToolBridge::new(Arc::new(
                AppendGameDesignProfileTool::new(provider.clone()),
            ))));
            tools.push(Arc::new(ToolBridge::new(Arc::new(
                AppendGameDesignLearningTool::new(provider.clone()),
            ))));
            tools.push(Arc::new(ToolBridge::new(Arc::new(
                SearchGameDesignLearningsTool::new(provider.clone()),
            ))));
            tools.push(Arc::new(ToolBridge::new(Arc::new(
                SetGameDesignLanguageTool::new(provider.clone()),
            ))));
            tools.push(Arc::new(ToolBridge::new(Arc::new(
                EnsureGameDesignRoutingTool::new(provider.clone()),
            ))));
            tools.push(Arc::new(ToolBridge::new(Arc::new(
                SyncGameDesignArtifactTool::new(provider.clone()),
            ))));
        }

        tools
    }
}

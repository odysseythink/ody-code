use std::env;
use std::fs::File;
use std::path::PathBuf;
use std::sync::Arc;

use agent_rs::background::manager::BackgroundManager;
use agent_rs::cron::manager::{CronManager, CronManagerOptions};
use agent_rs::records::nested::{GoalBudgetLimits, GoalStatus};
use agent_rs::turn::background_cron_driver::{run_fixture, BackgroundCronFixture};
use agent_rs::turn::fixture_agent::{FixtureAgent, FixtureTool};
use agent_rs::turn::types::LoopControl;
use agent_rs::turn::TurnFlow;
use anyhow::{Context, Error};

#[tokio::main]
async fn main() -> Result<(), Error> {
    let args: Vec<String> = env::args().collect();
    let fixture_path = args
        .get(1)
        .map(PathBuf::from)
        .context("usage: background_cron_l3 <fixture.json>")?;

    let file = File::open(&fixture_path)
        .with_context(|| format!("failed to open {}", fixture_path.display()))?;
    let fixture: BackgroundCronFixture = serde_json::from_reader(file)
        .with_context(|| format!("failed to parse {}", fixture_path.display()))?;

    let mut agent = FixtureAgent::new(fixture.responses.clone(), vec![]);

    if let Some(ref ctrl) = fixture.loop_control {
        agent.loop_control = Some(LoopControl {
            max_steps_per_turn: ctrl.max_steps,
            max_retries_per_step: ctrl.max_retry_attempts,
            reserved_context_size: None,
            ..Default::default()
        });
    }

    let agent = Arc::new(agent);

    let tools: Vec<_> = fixture
        .tools
        .clone()
        .into_iter()
        .map(|def| Arc::new(FixtureTool::new(def, agent.clone())) as Arc<_>)
        .collect();
    agent.tools.lock().unwrap().extend(tools);

    if let Some(ref goal) = fixture.initial_goal {
        let status = match goal.status.as_str() {
            "active" => GoalStatus::Active,
            "paused" => GoalStatus::Paused,
            "blocked" => GoalStatus::Blocked,
            "complete" => GoalStatus::Complete,
            _ => GoalStatus::Active,
        };
        agent.set_goal(
            status,
            GoalBudgetLimits {
                token_budget: goal.budget.token_budget,
                turn_budget: goal.budget.turn_budget,
                wall_clock_budget_ms: goal.budget.wall_clock_budget_ms,
            },
        );
    }

    let flow = Arc::new(TurnFlow::new(agent.clone()));
    let background = Arc::new(BackgroundManager::new(agent.clone(), flow.clone(), None));
    let cron = CronManager::new(
        agent.clone(),
        flow.clone(),
        None,
        CronManagerOptions {
            clocks: Some(agent.clock()),
            poll_interval_ms: Some(0),
        },
    );
    agent.install_managers(background, cron.clone());

    let snapshot = run_fixture(fixture, agent.clone(), flow.clone()).await?;
    println!("{}", serde_json::to_string_pretty(&snapshot)?);
    Ok(())
}

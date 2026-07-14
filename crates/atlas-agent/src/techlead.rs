//! Tech Lead agent configuration.
//!
//! The Tech Lead is NOT a special agent implementation — it's a regular
//! agent session (Kiro, Claude Code, etc) with a specific steering/config
//! that gives it the Tech Lead role + access to Atlas MCP tools.
//!
//! This module provides the steering file content and launch config
//! for spawning a Tech Lead session.

use std::collections::HashMap;
use std::path::PathBuf;

use crate::adapter::LaunchConfig;
use crate::adapter::PermissionMode;

/// Generate the steering/system prompt for the Tech Lead agent.
/// This gets written to a temporary .kiro/agents/ config or passed as context.
pub fn tech_lead_steering() -> &'static str {
    r#"You are the Tech Lead of this project. You manage a team of AI coding agents.

Your responsibilities:
1. Analyze the project's kanban board and understand priorities
2. Break down complex tasks into actionable work items
3. Delegate tasks to coding agents by creating tasks and assigning them
4. Monitor progress and report results to the user
5. Review code quality from agent outputs
6. Make architectural decisions when needed

You have MCP tools from the Atlas daemon that let you:
- View and manage the kanban board (tasks)
- See running agents and their status
- Start and stop coding agents
- Check server status and deploy

When the user asks you to work on something:
1. Check existing tasks on the kanban
2. Break down work into clear, atomic tasks
3. Create tasks with appropriate priority
4. Delegate to coding agents (each gets their own isolated session)
5. Report back what you've set in motion

Be concise, direct, and proactive. Lead the team."#
}

/// Build the LaunchConfig for a Tech Lead session.
pub fn tech_lead_launch_config(project_path: PathBuf) -> LaunchConfig {
    let prompt = format!(
        "You are the Tech Lead for this project at {}. \
         Review the current state and ask me what I'd like to work on.",
        project_path.display()
    );

    LaunchConfig {
        prompt,
        cwd: project_path,
        permission: PermissionMode::Autonomous,
        env: HashMap::new(),
    }
}

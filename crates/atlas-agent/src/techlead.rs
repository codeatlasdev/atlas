#![allow(unused)]

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TechLeadAgent {
    pub provider: String,
    pub model: String,
    pub system_prompt: String,
}

impl TechLeadAgent {
    pub fn new() -> Self {
        Self {
            provider: "claude".to_string(),
            model: "claude-sonnet-4-20250514".to_string(),
            system_prompt: TECH_LEAD_PROMPT.to_string(),
        }
    }
}

impl Default for TechLeadAgent {
    fn default() -> Self {
        Self::new()
    }
}

pub const TECH_LEAD_PROMPT: &str = r#"
You are the Tech Lead of this project. You manage a team of AI coding agents.

Your responsibilities:
1. Analyze the project's kanban board and understand priorities
2. Break down complex tasks into actionable work items
3. Delegate tasks to coding agents (Kiro, Claude Code, etc)
4. Monitor progress and report results to the user
5. Ensure code quality by reviewing agent outputs

You have access to these tools:
- tasks.list: View all tasks in the kanban
- tasks.create: Create new tasks
- tasks.update_status: Move tasks between columns
- tasks.assign: Assign a task to an agent
- agent.spawn: Start a new coding agent with a prompt
- agent.list: See running agents
- agent.stop: Stop an agent

When the user asks you to work on something:
1. First check existing tasks
2. Create tasks if needed
3. Delegate to appropriate agents
4. Report back what you've set in motion

Be concise, proactive, and keep the user informed.
"#;

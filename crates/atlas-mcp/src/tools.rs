use serde_json::{json, Value};

use crate::protocol::ToolDefinition;

pub fn all_tools() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "tasks_list".to_string(),
            description: "List kanban tasks for a project".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "project_path": {
                        "type": "string",
                        "description": "Path to the project directory"
                    },
                    "status": {
                        "type": "string",
                        "description": "Filter by status: todo, doing, done, blocked",
                        "enum": ["todo", "doing", "done", "blocked"]
                    }
                },
                "required": ["project_path"]
            }),
        },
        ToolDefinition {
            name: "tasks_create".to_string(),
            description: "Create a new kanban task".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "project_path": {
                        "type": "string",
                        "description": "Path to the project directory"
                    },
                    "title": {
                        "type": "string",
                        "description": "Task title"
                    },
                    "description": {
                        "type": "string",
                        "description": "Task description"
                    },
                    "priority": {
                        "type": "string",
                        "description": "Priority level",
                        "enum": ["low", "medium", "high", "critical"]
                    }
                },
                "required": ["project_path", "title"]
            }),
        },
        ToolDefinition {
            name: "tasks_update".to_string(),
            description: "Update task status".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "project_path": {
                        "type": "string",
                        "description": "Path to the project directory"
                    },
                    "task_id": {
                        "type": "string",
                        "description": "Task ID to update"
                    },
                    "status": {
                        "type": "string",
                        "description": "New status",
                        "enum": ["todo", "doing", "done", "blocked"]
                    }
                },
                "required": ["project_path", "task_id", "status"]
            }),
        },
        ToolDefinition {
            name: "agent_spawn".to_string(),
            description: "Spawn a coding agent to work on a task".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "project_path": {
                        "type": "string",
                        "description": "Path to the project directory"
                    },
                    "task": {
                        "type": "string",
                        "description": "Task description for the agent"
                    },
                    "provider": {
                        "type": "string",
                        "description": "AI provider to use",
                        "enum": ["claude", "kiro"]
                    }
                },
                "required": ["project_path", "task"]
            }),
        },
        ToolDefinition {
            name: "agent_list".to_string(),
            description: "List active coding agents".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "project_path": {
                        "type": "string",
                        "description": "Path to the project directory"
                    }
                },
                "required": ["project_path"]
            }),
        },
        ToolDefinition {
            name: "agent_stop".to_string(),
            description: "Stop a running coding agent".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_id": {
                        "type": "string",
                        "description": "Agent ID to stop"
                    }
                },
                "required": ["agent_id"]
            }),
        },
        ToolDefinition {
            name: "memory_search".to_string(),
            description: "Search project memory for relevant facts and context".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "project_path": {
                        "type": "string",
                        "description": "Path to the project directory"
                    },
                    "query": {
                        "type": "string",
                        "description": "Search query"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum results to return",
                        "default": 10
                    }
                },
                "required": ["project_path", "query"]
            }),
        },
        ToolDefinition {
            name: "memory_store".to_string(),
            description: "Store a new memory/fact in the project knowledge base".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "project_path": {
                        "type": "string",
                        "description": "Path to the project directory"
                    },
                    "content": {
                        "type": "string",
                        "description": "Content to store"
                    },
                    "category": {
                        "type": "string",
                        "description": "Category for the memory",
                        "enum": ["decision", "fact", "context", "preference"]
                    }
                },
                "required": ["project_path", "content"]
            }),
        },
        ToolDefinition {
            name: "servers_status".to_string(),
            description: "Check status of managed servers".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "server_id": {
                        "type": "string",
                        "description": "Specific server ID (optional, lists all if omitted)"
                    }
                }
            }),
        },
        ToolDefinition {
            name: "deploy_trigger".to_string(),
            description: "Trigger a deployment to a server".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "project_path": {
                        "type": "string",
                        "description": "Path to the project directory"
                    },
                    "server_id": {
                        "type": "string",
                        "description": "Target server ID"
                    },
                    "service": {
                        "type": "string",
                        "description": "Service name to deploy"
                    }
                },
                "required": ["project_path", "server_id", "service"]
            }),
        },
    ]
}

/// Maps MCP tool name to daemon JSON-RPC method and transforms arguments
pub fn map_tool_to_daemon(tool_name: &str, arguments: Value) -> Option<(&'static str, Value)> {
    match tool_name {
        "tasks_list" => Some(("tasks.list", arguments)),
        "tasks_create" => Some(("tasks.create", arguments)),
        "tasks_update" => Some(("tasks.update_status", arguments)),
        "agent_spawn" => Some(("agent.spawn", arguments)),
        "agent_list" => Some(("agent.list", arguments)),
        "agent_stop" => Some(("agent.stop", arguments)),
        "memory_search" => Some(("memory.search", arguments)),
        "memory_store" => Some(("memory.store", arguments)),
        "servers_status" => {
            if arguments.get("server_id").is_some() {
                Some(("servers.status", arguments))
            } else {
                Some(("servers.list", arguments))
            }
        }
        "deploy_trigger" => Some(("project.services.start", arguments)),
        _ => None,
    }
}

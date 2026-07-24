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
                    "title": {
                        "type": "string",
                        "description": "Short title for the session board (e.g. 'Monetization Research')"
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
            name: "agent_output".to_string(),
            description: "Get the latest output/response from a worker agent. Use to check what a worker has produced.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_id": {
                        "type": "string",
                        "description": "Agent session ID to query"
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
            name: "blackboard_write".to_string(),
            description: "Write a finding, decision, question, or progress update to the shared project blackboard. Other agents can read what you write.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "project_path": {
                        "type": "string",
                        "description": "Path to the project directory"
                    },
                    "type": {
                        "type": "string",
                        "description": "Entry type",
                        "enum": ["finding", "question", "decision", "progress", "request", "answer"]
                    },
                    "content": {
                        "type": "string",
                        "description": "The content to share with other agents"
                    },
                    "tags": {
                        "type": "array",
                        "items": {"type": "string"},
                        "description": "Tags for filtering (e.g. ['monetization', 'pricing'])"
                    }
                },
                "required": ["project_path", "type", "content"]
            }),
        },
        ToolDefinition {
            name: "blackboard_read".to_string(),
            description: "Read entries from the shared project blackboard. See what other agents have written — their findings, progress, questions, etc.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "project_path": {
                        "type": "string",
                        "description": "Path to the project directory"
                    },
                    "type": {
                        "type": "string",
                        "description": "Filter by entry type",
                        "enum": ["finding", "question", "decision", "progress", "request", "answer"]
                    },
                    "tags": {
                        "type": "array",
                        "items": {"type": "string"},
                        "description": "Filter by tags"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Max entries to return (default 20)"
                    }
                },
                "required": ["project_path"]
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
        "agent_spawn" => {
            // MCP tool params: {project_path, task, title, provider}
            // Daemon expects: {adapter, prompt, cwd, permission, title}
            let cwd = arguments.get("project_path")
                .and_then(|v| v.as_str())
                .unwrap_or("/tmp");
            let prompt = arguments.get("task")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let adapter = arguments.get("provider")
                .and_then(|v| v.as_str())
                .unwrap_or("kiro");
            let title = arguments.get("title")
                .and_then(|v| v.as_str());

            let mut params = json!({
                "adapter": adapter,
                "prompt": prompt,
                "cwd": cwd,
                "permission": "autonomous"
            });
            if let Some(t) = title {
                params["title"] = json!(t);
            }

            Some(("agent.spawn", params))
        }
        "agent_list" => Some(("agent.list", arguments)),
        "agent_stop" => Some(("agent.stop", arguments)),
        "agent_output" => {
            let agent_id = arguments.get("agent_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            Some(("agent.output", json!({"session_id": agent_id})))
        }
        "memory_search" => Some(("memory.search", arguments)),
        "memory_store" => Some(("memory.store", arguments)),
        "blackboard_write" => {
            let project_path = arguments.get("project_path").and_then(|v| v.as_str()).unwrap_or("");
            let entry_type = arguments.get("type").and_then(|v| v.as_str()).unwrap_or("finding");
            let content = arguments.get("content").and_then(|v| v.as_str()).unwrap_or("");
            let tags: Vec<String> = arguments.get("tags")
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                .unwrap_or_default();

            Some(("blackboard.write", json!({
                "project_path": project_path,
                "author": "agent",
                "type": entry_type,
                "content": content,
                "tags": tags
            })))
        }
        "blackboard_read" => {
            Some(("blackboard.read", arguments))
        }
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

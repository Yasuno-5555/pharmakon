use async_trait::async_trait;
use pharmakon_common::{AgentError, AgentResult, Tool, ToolCategory};
use serde_json::{Value, json};
use std::fs;
use std::path::Path;

pub struct TaskTrackerTool;

impl Default for TaskTrackerTool {
    fn default() -> Self {
        Self::new()
    }
}

impl TaskTrackerTool {
    pub fn new() -> Self {
        Self
    }

    fn get_task_file_path(&self) -> String {
        "task.md".to_string()
    }
}

#[async_trait]
impl Tool for TaskTrackerTool {
    fn name(&self) -> &str {
        "task_tracker"
    }

    fn description(&self) -> &str {
        "Manage the project task list (task.md). Use this to create, update, and track progress of complex tasks."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["init", "add", "update", "read", "graph"],
                    "description": "Action to perform on the task tracker."
                },
                "task_name": { "type": "string", "description": "Name of the task (for 'add' or 'update')" },
                "status": {
                    "type": "string",
                    "enum": ["todo", "in_progress", "done"],
                    "description": "Status of the task (for 'update')"
                },
                "dependencies": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "List of task names this task depends on (for 'add' or 'update')"
                },
                "content": { "type": "string", "description": "Full content for 'init'" }
            },
            "required": ["action"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Custom("project_management".to_string())
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let action = args["action"]
            .as_str()
            .ok_or_else(|| AgentError("Missing action".to_string()))?;
        let path = self.get_task_file_path();

        match action {
            "init" => {
                let content = args["content"]
                    .as_str()
                    .unwrap_or("# Task Tracker\n\n- [ ] Initial Task");
                fs::write(&path, content).map_err(|e| AgentError(e.to_string()))?;
                Ok(format!("✅ Task tracker initialized at {}", path))
            }
            "read" => {
                if !Path::new(&path).exists() {
                    return Ok(
                        "Task tracker file (task.md) does not exist yet. Use 'init' to create it."
                            .to_string(),
                    );
                }
                let content = fs::read_to_string(&path).map_err(|e| AgentError(e.to_string()))?;
                Ok(content)
            }
            "add" => {
                let task_name = args["task_name"]
                    .as_str()
                    .ok_or_else(|| AgentError("Missing task_name".to_string()))?;
                let dependencies = args["dependencies"].as_array();

                let mut content = if Path::new(&path).exists() {
                    fs::read_to_string(&path).map_err(|e| AgentError(e.to_string()))?
                } else {
                    "# Task Tracker\n\n".to_string()
                };

                let mut task_line = format!("- [ ] {}", task_name);
                if let Some(deps) = dependencies
                    && !deps.is_empty() {
                        let dep_list = deps
                            .iter()
                            .map(|v| v.as_str().unwrap_or_default())
                            .collect::<Vec<_>>()
                            .join(", ");
                        task_line.push_str(&format!(" (depends on: {})", dep_list));
                    }
                content.push_str(&format!("{}\n", task_line));
                fs::write(&path, content).map_err(|e| AgentError(e.to_string()))?;
                Ok(format!("✅ Added task: {}", task_name))
            }
            "update" => {
                let task_name = args["task_name"]
                    .as_str()
                    .ok_or_else(|| AgentError("Missing task_name".to_string()))?;
                let status = args["status"]
                    .as_str()
                    .ok_or_else(|| AgentError("Missing status".to_string()))?;

                if !Path::new(&path).exists() {
                    return Err(AgentError("Task tracker file does not exist.".to_string()));
                }

                let content = fs::read_to_string(&path).map_err(|e| AgentError(e.to_string()))?;
                let mark = match status {
                    "todo" => "[ ]",
                    "in_progress" => "[/]",
                    "done" => "[x]",
                    _ => "[ ]",
                };

                // Simple string replacement for now
                let new_content = content
                    .replace(
                        &format!("[ ] {}", task_name),
                        &format!("{} {}", mark, task_name),
                    )
                    .replace(
                        &format!("[/] {}", task_name),
                        &format!("{} {}", mark, task_name),
                    )
                    .replace(
                        &format!("[x] {}", task_name),
                        &format!("{} {}", mark, task_name),
                    );

                fs::write(&path, new_content).map_err(|e| AgentError(e.to_string()))?;
                Ok(format!("✅ Updated task '{}' to {}", task_name, status))
            }
            "graph" => {
                if !Path::new(&path).exists() {
                    return Err(AgentError("Task tracker file does not exist.".to_string()));
                }
                let content = fs::read_to_string(&path).map_err(|e| AgentError(e.to_string()))?;

                // Parse tasks and dependencies from markdown
                // This is a naive parser for the [ ] Task Name (depends on: A, B) format
                let mut mermaid = String::from("graph TD\n");
                for line in content.lines() {
                    if line.starts_with("- [") {
                        let name_part = &line[6..];
                        if let Some(dep_idx) = name_part.find("(depends on:") {
                            let task_name = name_part[..dep_idx].trim();
                            let deps_part = &name_part[dep_idx + 12..name_part.len() - 1];
                            for dep in deps_part.split(',') {
                                mermaid.push_str(&format!(
                                    "    {} --> {}\n",
                                    dep.trim(),
                                    task_name
                                ));
                            }
                        }
                    }
                }
                Ok(format!(
                    "### Task Dependency Graph\n\n```mermaid\n{}\n```",
                    mermaid
                ))
            }
            _ => Err(AgentError("Unknown action".to_string())),
        }
    }
}

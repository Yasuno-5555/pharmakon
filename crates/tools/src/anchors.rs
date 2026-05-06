use async_trait::async_trait;
use pharmakon_common::{AgentError, AgentResult, Tool};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::fs;
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Anchor {
    pub path: String,
    pub line: usize,
    pub label: String,
    pub description: String,
}

#[derive(Serialize, Deserialize, Default)]
struct AnchorDb {
    pub anchors: Vec<Anchor>,
}

pub struct SetAnchorTool;

#[async_trait]
impl Tool for SetAnchorTool {
    fn name(&self) -> &str {
        "set_semantic_anchor"
    }
    fn description(&self) -> &str {
        "Mark a specific line in a file with a semantic tag and description to help yourself and other agents navigate the codebase later."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Relative path to the file" },
                "line": { "type": "integer", "description": "Line number" },
                "label": { "type": "string", "description": "Short label (e.g. 'state_mutation')" },
                "description": { "type": "string", "description": "Detailed explanation of why this point is important" }
            },
            "required": ["path", "line", "label", "description"]
        })
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let path = args["path"].as_str().unwrap_or_default();
        let line = args["line"].as_u64().unwrap_or(0) as usize;
        let label = args["label"].as_str().unwrap_or_default();
        let description = args["description"].as_str().unwrap_or_default();

        let anchor = Anchor {
            path: path.to_string(),
            line,
            label: label.to_string(),
            description: description.to_string(),
        };

        let db_path = PathBuf::from(".pharmakon/anchors.json");
        if let Some(parent) = db_path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        let mut db = if db_path.exists() {
            let content = fs::read_to_string(&db_path).map_err(|e| AgentError(e.to_string()))?;
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            AnchorDb::default()
        };

        // Remove existing anchor for same path/line if exists
        db.anchors.retain(|a| a.path != path || a.line != line);
        db.anchors.push(anchor);

        let content = serde_json::to_string_pretty(&db).map_err(|e| AgentError(e.to_string()))?;
        fs::write(&db_path, content).map_err(|e| AgentError(e.to_string()))?;

        Ok(format!(
            "Successfully set semantic anchor '{}' at {}:{}",
            label, path, line
        ))
    }
}

pub struct ListAnchorsTool;

#[async_trait]
impl Tool for ListAnchorsTool {
    fn name(&self) -> &str {
        "list_semantic_anchors"
    }
    fn description(&self) -> &str {
        "List all semantic anchors set in this project to quickly understand the codebase structure and key logic points."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {}
        })
    }

    async fn call(&self, _args: Value) -> AgentResult<String> {
        let db_path = PathBuf::from(".pharmakon/anchors.json");
        if !db_path.exists() {
            return Ok("No semantic anchors found in this project.".to_string());
        }

        let content = fs::read_to_string(&db_path).map_err(|e| AgentError(e.to_string()))?;
        let db: AnchorDb = serde_json::from_str(&content).map_err(|e| AgentError(e.to_string()))?;

        if db.anchors.is_empty() {
            return Ok("No semantic anchors found in this project.".to_string());
        }

        let mut output = String::from("### Semantic Anchors\n\n");
        for a in db.anchors {
            output.push_str(&format!(
                "- **{}** ({}:{}): {}\n",
                a.label, a.path, a.line, a.description
            ));
        }

        Ok(output)
    }
}

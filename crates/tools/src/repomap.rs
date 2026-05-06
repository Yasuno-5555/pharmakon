use async_trait::async_trait;
use pharmakon_common::{AgentError, AgentResult, Tool, ToolCategory};
use serde_json::{Value, json};
use std::fs;
use std::path::Path;
use tree_sitter::{Parser, Query, QueryCursor};

pub struct RepoMapTool;

impl RepoMapTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for RepoMapTool {
    fn name(&self) -> &str {
        "get_repo_map"
    }
    fn description(&self) -> &str {
        "Generate a concise map of the repository's structure, including function signatures, structs, and traits using AST analysis. Helps in understanding the codebase layout."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "default": ".", "description": "Directory to map" }
            }
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::FileSystem
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let path = args["path"].as_str().unwrap_or(".");
        let mut report = format!("### Repo Map: {}\n\n", path);

        let walker = ignore::WalkBuilder::new(path)
            .hidden(false)
            .git_ignore(true)
            .build();

        for entry in walker.flatten() {
            let p = entry.path();
            if p.is_file() && p.extension().is_some_and(|ext| ext == "rs") {
                if let Ok(symbols) = self.extract_symbols(p) {
                    if !symbols.is_empty() {
                        report.push_str(&format!("#### {}\n", p.display()));
                        for sym in symbols {
                            report.push_str(&format!("- {}\n", sym));
                        }
                        report.push('\n');
                    }
                }
            }
        }

        Ok(report)
    }
}

impl RepoMapTool {
    fn extract_symbols(&self, path: &Path) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        let code = fs::read_to_string(path)?;
        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_rust::language())?;
        let tree = parser.parse(&code, None).ok_or("Failed to parse")?;

        // Query for functions, structs, traits, and enums
        let query_str = "
            (function_item name: (identifier) @fn_name)
            (struct_item name: (type_identifier) @struct_name)
            (trait_item name: (type_identifier) @trait_name)
            (enum_item name: (type_identifier) @enum_name)
            (impl_item type: (type_identifier) @impl_name)
        ";
        let query = Query::new(&tree_sitter_rust::language(), query_str)?;
        let mut cursor = QueryCursor::new();
        let matches = cursor.matches(&query, tree.root_node(), code.as_bytes());

        let mut symbols = Vec::new();
        for m in matches {
            for capture in m.captures {
                let node = capture.node;
                let kind = match capture.index {
                    0 => "fn",
                    1 => "struct",
                    2 => "trait",
                    3 => "enum",
                    4 => "impl",
                    _ => "item",
                };
                let name = &code[node.byte_range()];
                symbols.push(format!("{} {}", kind, name));
            }
        }
        symbols.sort();
        symbols.dedup();
        Ok(symbols)
    }
}

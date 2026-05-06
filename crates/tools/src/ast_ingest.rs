use async_trait::async_trait;
use pharmakon_common::{AgentError, AgentResult, Tool, ToolCategory};
use serde_json::{Value, json};
use std::fs;
use std::path::Path;
use std::sync::Arc;
use tree_sitter::{Parser, Query, QueryCursor};

pub struct ASTKnowledgeIngestTool {
    pub nexus: Arc<pharmakon_memory::weaver::KnowledgeNexus>,
}

impl ASTKnowledgeIngestTool {
    pub fn new(nexus: Arc<pharmakon_memory::weaver::KnowledgeNexus>) -> Self {
        Self { nexus }
    }

    fn extract_blocks(
        &self,
        code: &str,
    ) -> Result<Vec<KnowledgeBlock>, Box<dyn std::error::Error>> {
        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_rust::language())?;
        let tree = parser.parse(code, None).ok_or("Failed to parse")?;

        let query_str = "
            (function_item) @fn
            (struct_item) @struct
            (trait_item) @trait
            (enum_item) @enum
            (impl_item) @impl
        ";
        let query = Query::new(&tree_sitter_rust::language(), query_str)?;
        let mut cursor = QueryCursor::new();
        let matches = cursor.matches(&query, tree.root_node(), code.as_bytes());

        let mut blocks = Vec::new();
        for m in matches {
            for capture in m.captures {
                let node = capture.node;
                let kind = match capture.index {
                    0 => "function",
                    1 => "struct",
                    2 => "trait",
                    3 => "enum",
                    4 => "impl",
                    _ => "block",
                };

                let content = &code[node.byte_range()];

                // Try to find a name for the block
                let name_query_str = "(identifier) @id";
                let name_query = Query::new(&tree_sitter_rust::language(), name_query_str)?;
                let mut name_cursor = QueryCursor::new();
                let name = name_cursor
                    .matches(&name_query, node, code.as_bytes())
                    .next()
                    .and_then(|m| m.captures.first())
                    .map(|c| &code[c.node.byte_range()])
                    .unwrap_or("unnamed")
                    .to_string();

                blocks.push(KnowledgeBlock {
                    name,
                    kind: kind.to_string(),
                    content: content.to_string(),
                    range: node.start_position().row..node.end_position().row,
                });
            }
        }
        Ok(blocks)
    }
}

struct KnowledgeBlock {
    name: String,
    kind: String,
    content: String,
    range: std::ops::Range<usize>,
}

#[async_trait]
impl Tool for ASTKnowledgeIngestTool {
    fn name(&self) -> &str {
        "ingest_ast_knowledge"
    }
    fn description(&self) -> &str {
        "Parse a file into logical blocks (functions, structs) and index them into Knowledge Nexus for structural search."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Path to the file to ingest" }
            },
            "required": ["path"]
        })
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let path_str = args["path"]
            .as_str()
            .ok_or_else(|| AgentError("Missing path".to_string()))?;
        let path = Path::new(path_str);

        let code = fs::read_to_string(path)
            .map_err(|e| AgentError(format!("Failed to read file: {}", e)))?;

        let blocks = match path.extension().and_then(|s| s.to_str()) {
            Some("rs") => self
                .extract_blocks(&code)
                .map_err(|e| AgentError(format!("AST extraction failed: {}", e)))?,
            _ => {
                // Fallback for non-Rust files: Paragraph-based chunking
                code.split("\n\n")
                    .enumerate()
                    .map(|(i, chunk)| KnowledgeBlock {
                        name: format!("chunk_{}", i),
                        kind: "text_block".to_string(),
                        content: chunk.to_string(),
                        range: 0..0, // Range not easily calculated here
                    })
                    .collect()
            }
        };

        let mut nexus_entries = Vec::new();
        let mut report = format!("Ingested {} blocks from {}:\n", blocks.len(), path_str);

        for block in blocks {
            let id = format!("{}:{}#{}", path_str, block.name, block.kind);

            // 1. Vector Entry (handled via remember_batch sync)
            nexus_entries.push((id.clone(), block.content.clone()));

            // 2. Graph Entry
            let _ = self
                .nexus
                .graph
                .add_node(pharmakon_memory::graph::Node {
                    id: id.clone(),
                    label: format!("{}: {}", block.kind, block.name),
                    node_type: format!("code_{}", block.kind),
                    content: block.content.clone(),
                    summary: Some(format!("{} definition in {}", block.kind, path_str)),
                    embedding_id: Some(id.clone()),
                    embedding_status: "PENDING".to_string(),
                    access_count: 0,
                    last_access_time: chrono::Utc::now().timestamp(),
                    properties: json!({
                        "file": path_str,
                        "kind": block.kind,
                        "name": block.name,
                        "line_start": block.range.start,
                        "line_end": block.range.end,
                    }),
                })
                .await;

            // 3. File Linkage (Edge)
            let _ = self
                .nexus
                .graph
                .add_edge(pharmakon_memory::graph::Edge {
                    from_id: path_str.to_string(),
                    to_id: id.clone(),
                    relation: "contains".to_string(),
                    weight: 1.0,
                    metadata: json!({}),
                })
                .await;

            report.push_str(&format!("- [{}] {}\n", block.kind, block.name));
        }

        // Ensure the file itself is a node
        let _ = self
            .nexus
            .graph
            .add_node(pharmakon_memory::graph::Node {
                id: path_str.to_string(),
                label: format!("file: {}", path_str),
                node_type: "file".to_string(),
                content: path_str.to_string(),
                summary: Some(format!("Source file: {}", path_str)),
                embedding_id: None,
                embedding_status: "COMPLETED".to_string(), // File names don't necessarily need embedding
                access_count: 0,
                last_access_time: chrono::Utc::now().timestamp(),
                properties: json!({"type": "file"}),
            })
            .await;

        self.nexus
            .remember_batch(nexus_entries)
            .await
            .map_err(|e| AgentError(format!("Nexus indexing failed: {}", e)))?;

        Ok(report)
    }
}

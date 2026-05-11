use async_trait::async_trait;
use pharmakon_common::{AgentError, AgentResult, Tool, ToolCategory};
use serde_json::{json, Value};
use crate::codex_utils::{estimate_tokens};

pub struct ContextBudgetOptimizerTool;

#[async_trait]
impl Tool for ContextBudgetOptimizerTool {
    fn name(&self) -> &str {
        "context_budget_optimizer"
    }

    fn description(&self) -> &str {
        "Select the highest-value context items within a token budget using relevance, recency, importance, reliability, and pinned state."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "budget_tokens": { "type": "integer", "default": 4096 },
                "items": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "id": { "type": "string" },
                            "content": { "type": "string" },
                            "tokens": { "type": "integer" },
                            "relevance": { "type": "number" },
                            "recency": { "type": "number" },
                            "importance": { "type": "number" },
                            "reliability": { "type": "number" },
                            "pinned": { "type": "boolean" }
                        }
                    }
                }
            },
            "required": ["items"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::System
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let budget = args["budget_tokens"].as_u64().unwrap_or(4096) as usize;
        let items = args["items"]
            .as_array()
            .ok_or_else(|| AgentError("Missing items".to_string()))?;
        let mut scored = Vec::new();
        for (idx, item) in items.iter().enumerate() {
            let content = item["content"].as_str().unwrap_or_default();
            let tokens = item["tokens"]
                .as_u64()
                .map(|n| n as usize)
                .unwrap_or_else(|| estimate_tokens(content));
            let relevance = item["relevance"].as_f64().unwrap_or(0.5);
            let recency = item["recency"].as_f64().unwrap_or(0.5);
            let importance = item["importance"].as_f64().unwrap_or(0.5);
            let reliability = item["reliability"].as_f64().unwrap_or(0.7);
            let pinned = item["pinned"].as_bool().unwrap_or(false);
            let score = if pinned {
                10_000.0 + importance
            } else {
                (0.45 * relevance + 0.25 * importance + 0.20 * recency + 0.10 * reliability)
                    / (tokens.max(1) as f64).sqrt()
            };
            scored.push((idx, tokens, score, item.clone()));
        }
        scored.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
        let mut used = 0usize;
        let mut selected = Vec::new();
        let mut rejected = Vec::new();
        for (idx, tokens, score, item) in scored {
            let id = item["id"]
                .as_str()
                .map(|s| s.to_string())
                .unwrap_or_else(|| idx.to_string());
            if used + tokens <= budget || item["pinned"].as_bool().unwrap_or(false) {
                used += tokens;
                selected.push(json!({ "id": id, "tokens": tokens, "score": score, "item": item }));
            } else {
                rejected.push(json!({ "id": id, "tokens": tokens, "score": score }));
            }
        }
        Ok(json!({ "budget_tokens": budget, "used_tokens": used, "selected": selected, "rejected": rejected }).to_string())
    }
}

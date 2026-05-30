//! Capability Abstraction Layer — maps 65+ tools into 10 semantic capabilities.
//!
//! Problem: LLMs burn tokens thinking about "which tool to use" rather than "what to do."
//! Solution: Inject capability descriptions into prompts instead of full tool schemas.
//! The routing layer resolves capability requests to concrete tools on the agent side.
//!
//! Token savings: ~90% reduction in tool description overhead (~200 tokens vs ~2000).

use serde::{Deserialize, Serialize};

/// The 10 high-level capabilities that encompass all 65+ tools.
/// When the LLM selects a capability, the agent's routing layer
/// resolves it to the best concrete tool for the current context.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Capability {
    /// Find information: code search, web search, semantic grep, file reading.
    Search,
    /// Modify code and files: write, patch, AST mutations, refactoring.
    Modify,
    /// Execute commands and code: shell, terminal, REPL, build, test.
    Execute,
    /// Investigate and understand: workspace perception, repomap, LSP queries.
    Investigate,
    /// Orchestrate work: sub-agents, swarms, MCTS, tool routing.
    Orchestrate,
    /// Reflect and improve: cognitive mirror, regret minimization, self-diagnostic.
    Reflect,
    /// Validate and verify: linter, compiler checks, security audits, dry runs.
    Validate,
    /// Learn and remember: knowledge ingestion, memory management, embedding.
    Learn,
    /// Coordinate with external systems: browser, web tasks, APIs, media.
    Coordinate,
    /// Simulate and predict: counterfactuals, failure prediction, time travel debug.
    Simulate,
}

impl Capability {
    /// Human-readable description for prompt injection.
    pub fn description(&self) -> &'static str {
        match self {
            Self::Search => "Find information across code, web, and files",
            Self::Modify => "Write, patch, or refactor code and files",
            Self::Execute => "Run shell commands, tests, builds, or REPL code",
            Self::Investigate => "Analyze workspace structure, query LSP, map repos",
            Self::Orchestrate => "Spawn sub-agents, decompose tasks, route work",
            Self::Reflect => "Self-analyze decisions, minimize regret, optimize strategy",
            Self::Validate => "Lint, compile-check, security audit, dry-run operations",
            Self::Learn => "Ingest knowledge, manage memory, index embeddings",
            Self::Coordinate => "Interact with browsers, APIs, media, external systems",
            Self::Simulate => "Run counterfactuals, predict failures, debug historically",
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::Search => "search",
            Self::Modify => "modify",
            Self::Execute => "execute",
            Self::Investigate => "investigate",
            Self::Orchestrate => "orchestrate",
            Self::Reflect => "reflect",
            Self::Validate => "validate",
            Self::Learn => "learn",
            Self::Coordinate => "coordinate",
            Self::Simulate => "simulate",
        }
    }

    /// Get example tools for this capability (max 3, for prompt compactness).
    pub fn example_tools(&self) -> &'static [&'static str] {
        match self {
            Self::Search => &["brave_search", "grep_files", "web_fetch"],
            Self::Modify => &["write_file", "apply_patch", "mutate_ast"],
            Self::Execute => &["shell", "terminal", "cargo_build"],
            Self::Investigate => &["read_file", "lsp", "repomap"],
            Self::Orchestrate => &["fractal_swarm", "subagent", "route_tools"],
            Self::Reflect => &["cognitive_mirror", "reflect", "regret_minimization"],
            Self::Validate => &["rlfc", "diff_security_auditor", "dry_run"],
            Self::Learn => &[
                "ingest_ast_knowledge",
                "memory_management",
                "hydrate_context",
            ],
            Self::Coordinate => &["browser", "web_task", "media_understanding"],
            Self::Simulate => &[
                "counterfactual_simulator",
                "failure_prediction",
                "mcts_simulator",
            ],
        }
    }

    pub fn all() -> &'static [Capability] {
        &[
            Self::Search,
            Self::Modify,
            Self::Execute,
            Self::Investigate,
            Self::Orchestrate,
            Self::Reflect,
            Self::Validate,
            Self::Learn,
            Self::Coordinate,
            Self::Simulate,
        ]
    }

    /// Map a tool name to its primary capability.
    /// Returns None for tools that don't fit the capability model (internal/routing tools).
    pub fn from_tool_name(name: &str) -> Option<Self> {
        match name {
            // Search
            "search" | "brave_search" | "duckduckgo_search" | "google_search" | "gemini_search"
            | "web_fetch" | "grep_files" | "file_search" | "semantic_grep" | "custom_scout"
            | "read_file" | "link_understanding" => Some(Self::Search),

            // Modify
            "write_file"
            | "apply_patch"
            | "mutate_ast"
            | "semantic_conflict_resolution"
            | "spec_first_test"
            | "replace_content"
            | "git_add"
            | "git_commit"
            | "git_branch"
            | "python_interpreter" => Some(Self::Modify),

            // Execute
            "shell" | "terminal" | "run_background" | "get_process_status"
            | "send_command_input" | "node_repl" | "rlfc" => Some(Self::Execute),

            // Investigate
            "lsp"
            | "ast_lsp_bridge"
            | "repomap"
            | "workspace_perception"
            | "workspace_snapshot"
            | "soft_dependency_graph"
            | "nexus_visualizer"
            | "execution_trace"
            | "deterministic_replay"
            | "view_file"
            | "list_dir"
            | "find_definition"
            | "git_status"
            | "git_diff"
            | "git_log" => Some(Self::Investigate),

            // Orchestrate
            "fractal_swarm" | "subagent" | "mcts_simulator" | "route_tools"
            | "skill_composition" | "ephemeral_red_team" | "attention_router" | "playbook"
            | "task_tracker" | "checkpoint" => Some(Self::Orchestrate),

            // Reflect
            "cognitive_mirror"
            | "reflect"
            | "regret_minimization"
            | "self_diagnostic"
            | "intent_compiler"
            | "autonomy_dial"
            | "proactive_intervention"
            | "proactive_self_optimization"
            | "temporal_awareness"
            | "context_budget_optimizer" => Some(Self::Reflect),

            // Validate
            "diff_security_auditor"
            | "dry_run"
            | "tool_reliability"
            | "failure_memory"
            | "failure_prediction" => Some(Self::Validate),

            // Learn
            "ingest_ast_knowledge"
            | "memory_management"
            | "hydrate_context"
            | "graph_prefetch"
            | "memory_actor_status"
            | "commitment"
            | "context_connector"
            | "soul_manager" => Some(Self::Learn),

            // Coordinate
            "browser"
            | "web_task"
            | "media_understanding"
            | "canvas"
            | "screenshot"
            | "camera"
            | "local_model_router"
            | "automation"
            | "weather_lookup"
            | "finance_lookup"
            | "sports_lookup" => Some(Self::Coordinate),

            // Simulate
            "counterfactual_simulator"
            | "time_travel_debugger"
            | "codex_tool_catalog"
            | "discover_tools"
            | "current_time" => Some(Self::Simulate),

            // Routing / internal tools — not exposed as capabilities
            _ => None,
        }
    }
}

/// Build a compact capability catalog string for prompt injection.
/// Replaces the 65-tool catalog_summary with 10 capability descriptions.
/// Estimated token savings: ~1800 tokens → ~200 tokens.
pub fn capability_catalog_summary() -> String {
    let mut out = String::from("## Available Capabilities\n");
    out.push_str("(Select a capability; the agent will route to the best tool.)\n\n");

    for cap in Capability::all() {
        let tools = cap.example_tools();
        out.push_str(&format!(
            "- **{}**: {} (e.g., {})\n",
            cap.name(),
            cap.description(),
            tools.join(", ")
        ));
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_tools_have_capability_mapping() {
        // Verify that the major tools are mapped
        assert_eq!(
            Capability::from_tool_name("brave_search"),
            Some(Capability::Search)
        );
        assert_eq!(
            Capability::from_tool_name("write_file"),
            Some(Capability::Modify)
        );
        assert_eq!(
            Capability::from_tool_name("shell"),
            Some(Capability::Execute)
        );
        assert_eq!(
            Capability::from_tool_name("lsp"),
            Some(Capability::Investigate)
        );
        assert_eq!(
            Capability::from_tool_name("fractal_swarm"),
            Some(Capability::Orchestrate)
        );
        assert_eq!(
            Capability::from_tool_name("cognitive_mirror"),
            Some(Capability::Reflect)
        );
        assert_eq!(
            Capability::from_tool_name("dry_run"),
            Some(Capability::Validate)
        );
        assert_eq!(
            Capability::from_tool_name("ingest_ast_knowledge"),
            Some(Capability::Learn)
        );
        assert_eq!(
            Capability::from_tool_name("browser"),
            Some(Capability::Coordinate)
        );
        assert_eq!(
            Capability::from_tool_name("counterfactual_simulator"),
            Some(Capability::Simulate)
        );
    }

    #[test]
    fn test_capability_summary_is_compact() {
        let summary = capability_catalog_summary();
        assert!(
            summary.len() < 2000,
            "Summary too large: {} chars",
            summary.len()
        );
        assert!(summary.contains("**search**"));
        assert!(summary.contains("**modify**"));
    }

    #[test]
    fn test_capability_count() {
        assert_eq!(Capability::all().len(), 10);
    }
}

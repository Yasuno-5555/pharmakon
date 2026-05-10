//! Tool-specific catalog population.
//!
//! The `ToolMetaCatalog` struct lives in `pharmakon-common`.
//! This module provides `build_default_catalog()` which populates it
//! with all known tools and their ExecutionProfiles.

pub use pharmakon_common::tool_meta_catalog::{SearchResult, ToolMetaCatalog};
use pharmakon_common::{
    ExecutionProfile, FilesystemScope, Reversibility, SideEffectLevel, ToolCategory, ToolMeta,
};

// --- Profile presets ---

fn pure() -> ExecutionProfile {
    ExecutionProfile::default()
}

fn read_fs() -> ExecutionProfile {
    ExecutionProfile {
        side_effect_level: SideEffectLevel::None,
        filesystem_scope: FilesystemScope::Confined,
        reversibility: Reversibility::Trivial,
        ..Default::default()
    }
}

fn local_fs() -> ExecutionProfile {
    ExecutionProfile {
        side_effect_level: SideEffectLevel::Local,
        filesystem_scope: FilesystemScope::Confined,
        reversibility: Reversibility::Possible,
        ..Default::default()
    }
}

fn network() -> ExecutionProfile {
    ExecutionProfile {
        side_effect_level: SideEffectLevel::None,
        network_access: true,
        reversibility: Reversibility::Trivial,
        ..Default::default()
    }
}

fn dangerous_net() -> ExecutionProfile {
    ExecutionProfile {
        side_effect_level: SideEffectLevel::Irreversible,
        network_access: true,
        reversibility: Reversibility::Impractical,
        requires_human_approval: true,
        ..Default::default()
    }
}

fn dangerous_shell() -> ExecutionProfile {
    ExecutionProfile {
        side_effect_level: SideEffectLevel::Irreversible,
        filesystem_scope: FilesystemScope::Unrestricted,
        reversibility: Reversibility::Possible,
        requires_human_approval: true,
        ..Default::default()
    }
}

fn autonomous() -> ExecutionProfile {
    ExecutionProfile {
        side_effect_level: SideEffectLevel::Irreversible,
        network_access: false,
        filesystem_scope: FilesystemScope::None,
        reversibility: Reversibility::Impractical,
        requires_human_approval: true,
    }
}

/// Build the default catalog from all known tools.
pub fn build_default_catalog() -> ToolMetaCatalog {
    let entries = vec![
        // --- Core ---
        meta("current_time", "Get the current date and time", ToolCategory::Core, pure()),
        meta("discover_tools", "Search for available tools based on a query", ToolCategory::Core, pure()),
        meta("route_tools", "Activate tool categories to expand available capabilities", ToolCategory::Core, pure()),
        meta("self_diagnostic", "Run system diagnostics and health checks", ToolCategory::Core, pure()),
        meta("playbook", "Execute a saved procedural playbook", ToolCategory::Core, pure()),
        // --- FileSystem ---
        meta("read_file", "Read the contents of a file at a given path", ToolCategory::FileSystem, read_fs()),
        meta("write_file", "Write or overwrite content to a file", ToolCategory::FileSystem, local_fs()),
        meta("apply_patch", "Apply a unified diff patch to modify a file", ToolCategory::FileSystem, local_fs()),
        meta("repomap", "Generate a structural map of the repository", ToolCategory::FileSystem, read_fs()),
        meta("workspace_perception", "Scan workspace structure and detect project type", ToolCategory::FileSystem, read_fs()),
        meta("semantic_grep", "Search code using semantic patterns and AST awareness", ToolCategory::FileSystem, read_fs()),
        meta("custom_scout", "Deep recursive file search with pattern matching", ToolCategory::FileSystem, read_fs()),
        // --- Network ---
        meta("browser", "Open and interact with web pages in a headless browser", ToolCategory::Network, network()),
        meta("web_fetch", "Fetch and extract content from a URL", ToolCategory::Network, network()),
        meta("brave_search", "Search the web using Brave Search API", ToolCategory::Network, network()),
        meta("google_search", "Search the web using Google Search", ToolCategory::Network, network()),
        meta("search", "Search the web (auto-routes to best free backend)", ToolCategory::Network, network()),
        meta("duckduckgo_search", "Search the web using DuckDuckGo (free, no API key needed)", ToolCategory::Network, network()),
        meta("web_task", "Execute a complex multi-step web research task", ToolCategory::Network, dangerous_net()),
        meta("link_understanding", "Analyze and extract structured data from a URL", ToolCategory::Network, network()),
        // --- System ---
        meta("shell", "Execute a shell command and return output", ToolCategory::System, dangerous_shell()),
        meta("terminal", "Interactive terminal session management", ToolCategory::System, dangerous_shell()),
        meta("node_repl", "Execute JavaScript/TypeScript in a Node.js REPL", ToolCategory::System, dangerous_shell()),
        meta("checkpoint", "Save a checkpoint of current agent state", ToolCategory::System, local_fs()),
        // --- Coding ---
        meta("lsp", "Query Language Server Protocol for code intelligence", ToolCategory::Coding, pure()),
        meta("ast_lsp_bridge", "Bridge between AST analysis and LSP queries", ToolCategory::Coding, pure()),
        meta("spec_first_test", "Generate tests from specification before implementation", ToolCategory::Coding, local_fs()),
        meta("semantic_conflict_resolution", "Resolve merge conflicts using semantic understanding", ToolCategory::Coding, local_fs()),
        meta("diff_security_auditor", "Audit code diffs for security vulnerabilities", ToolCategory::Coding, pure()),
        meta("mutate_ast", "Perform precise AST-level code mutations", ToolCategory::Coding, local_fs()),
        meta("rlfc", "Reinforcement Learning from Compiler feedback optimization loop", ToolCategory::Coding, dangerous_shell()),
        // --- Autonomous ---
        meta("cognitive_mirror", "Reflect on reasoning patterns and biases", ToolCategory::Autonomous, pure()),
        meta("intent_compiler", "Compile user intent into structured execution plan", ToolCategory::Autonomous, pure()),
        meta("attention_router", "Route attention to most productive task area", ToolCategory::Autonomous, pure()),
        meta("regret_minimization", "Evaluate past decisions for regret-optimal future choices", ToolCategory::Autonomous, pure()),
        meta("counterfactual_simulator", "Simulate alternative approaches to compare outcomes", ToolCategory::Autonomous, pure()),
        meta("reflect", "Perform deep self-reflection on recent actions and outcomes", ToolCategory::Autonomous, pure()),
        meta("autonomy_dial", "Adjust autonomous operation level", ToolCategory::Autonomous, pure()),
        meta("failure_prediction", "Predict likelihood of failure for planned actions", ToolCategory::Autonomous, pure()),
        meta("proactive_intervention", "Proactively suggest improvements before problems occur", ToolCategory::Autonomous, pure()),
        meta("proactive_self_optimization", "Self-optimize internal processes for efficiency", ToolCategory::Autonomous, autonomous()),
        meta("hydrate_context", "Load relevant context from knowledge base into working memory", ToolCategory::Autonomous, pure()),
        meta("memory_management", "Manage long-term memory entries", ToolCategory::Autonomous, local_fs()),
        meta("ingest_ast_knowledge", "Parse source code AST and index into knowledge graph", ToolCategory::Autonomous, local_fs()),
        meta("graph_prefetch", "Prefetch related knowledge graph nodes for upcoming tasks", ToolCategory::Autonomous, pure()),
        meta("memory_actor_status", "Query the memory subsystem health and statistics", ToolCategory::Autonomous, pure()),
        meta("failure_memory", "Record and query past failure patterns", ToolCategory::Autonomous, local_fs()),
        // --- Orchestration ---
        meta("fractal_swarm", "Decompose task into parallel sub-agent work packets", ToolCategory::Orchestration, autonomous()),
        meta("subagent", "Spawn a specialized sub-agent for a focused task", ToolCategory::Orchestration, autonomous()),
        meta("mcts_simulator", "Monte Carlo Tree Search simulation of implementation options", ToolCategory::Orchestration, dangerous_shell()),
        meta("skill_composition", "Compose multiple tools into a reusable skill sequence", ToolCategory::Orchestration, pure()),
        meta("ephemeral_red_team", "Spawn an adversarial agent to challenge assumptions", ToolCategory::Orchestration, autonomous()),
        // --- Observability ---
        meta("execution_trace", "Record and analyze tool execution traces", ToolCategory::Core, pure()),
        meta("deterministic_replay", "Replay a previous execution trace step by step", ToolCategory::Core, pure()),
        meta("tool_reliability", "Score tool reliability based on historical success rates", ToolCategory::Core, pure()),
        meta("context_budget_optimizer", "Optimize context window token allocation", ToolCategory::Core, pure()),
        meta("dry_run", "Simulate tool execution without side effects", ToolCategory::Core, pure()),
        meta("workspace_snapshot", "Capture full workspace state for comparison", ToolCategory::Core, local_fs()),
        meta("local_model_router", "Route requests to the optimal model based on task type", ToolCategory::Core, pure()),
        meta("temporal_awareness", "Track time-based patterns and scheduling", ToolCategory::Core, pure()),
        meta("soft_dependency_graph", "Map implicit dependencies between components", ToolCategory::Core, pure()),
        meta("nexus_visualizer", "Visualize the knowledge graph structure", ToolCategory::Core, pure()),
        meta("time_travel_debugger", "Debug by stepping through historical state", ToolCategory::Core, pure()),
        meta("codex_tool_catalog", "List all available tools with descriptions", ToolCategory::Core, pure()),
        // --- Media ---
        meta("screenshot", "Capture a screenshot of the current screen", ToolCategory::Media, pure()),
        meta("camera", "Capture image from camera", ToolCategory::Media, pure()),
        meta("media_understanding", "Analyze images and media using vision model", ToolCategory::Media, pure()),
        meta("canvas", "Draw interactive visualizations on a shared canvas", ToolCategory::Media, local_fs()),
        // --- Misc ---
        meta("task_tracker", "Track and manage project tasks and milestones", ToolCategory::Core, local_fs()),
        meta("commitment", "Record and track commitments and promises", ToolCategory::Core, local_fs()),
        meta("context_connector", "Connect to external knowledge sources", ToolCategory::Core, pure()),
        meta("soul_manager", "Modify agent personality and behavioral traits", ToolCategory::Core, local_fs()),
        meta("automation", "Schedule and manage automated recurring tasks", ToolCategory::Core, dangerous_shell()),
        meta("weather_lookup", "Get current weather information", ToolCategory::Network, network()),
        meta("finance_lookup", "Get financial market data", ToolCategory::Network, network()),
        meta("sports_lookup", "Get sports scores and information", ToolCategory::Network, network()),
        meta("codeact", "Execute a Rhai script for compound multi-tool operations in one call", ToolCategory::Core, local_fs()),
    ];

    ToolMetaCatalog::new(entries)
}

fn meta(name: &str, desc: &str, cat: ToolCategory, profile: ExecutionProfile) -> ToolMeta {
    ToolMeta {
        name: name.to_string(),
        description: desc.to_string(),
        category: cat,
        profile,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bm25_search_finds_relevant_tools() {
        let catalog = build_default_catalog();
        let results = catalog.search("search web documentation", 5);
        let names: Vec<&str> = results.iter().map(|r| r.meta.name.as_str()).collect();
        assert!(
            names.iter().any(|n| ["brave_search", "google_search", "web_fetch", "browser"].contains(n)),
            "Should find web tools, got: {:?}", names
        );
    }

    #[test]
    fn test_bm25_exact_name_boost() {
        let catalog = build_default_catalog();
        let results = catalog.search("shell", 5);
        assert_eq!(results[0].meta.name, "shell", "Exact name match should be first");
    }

    #[test]
    fn test_catalog_summary_compact() {
        let catalog = build_default_catalog();
        let summary = catalog.catalog_summary();
        assert!(summary.contains("[core]"));
        assert!(summary.len() < 6000, "Summary should be compact: {} chars", summary.len());
    }

    #[test]
    fn test_by_category() {
        let catalog = build_default_catalog();
        let network = catalog.by_category(&ToolCategory::Network);
        assert!(network.len() >= 4);
    }
}
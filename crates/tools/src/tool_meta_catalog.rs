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
        meta("discover_tools", "Search for available tools based on a query", ToolCategory::Core, pure()),
        meta("route_tools", "Activate tool categories to expand available capabilities", ToolCategory::Core, pure()),
        meta("self_diagnostic", "Run system diagnostics and health checks", ToolCategory::Core, pure()),
        meta("playbook", "Execute a saved procedural playbook", ToolCategory::Core, pure()),
        // --- FileSystem ---
        meta("replace_content", "Strictly replace content chunks in a file safely", ToolCategory::FileSystem, local_fs()),
        meta("view_file", "Read file contents with line numbers and page support", ToolCategory::FileSystem, read_fs()),
        meta("list_dir", "List directory contents with gitignore awareness and depth control", ToolCategory::FileSystem, read_fs()),
        meta("read_file", "Read file contents with auto-detection of text/binary/image", ToolCategory::FileSystem, read_fs()),
        meta("write_file", "Write content to file with backup and directory creation", ToolCategory::FileSystem, local_fs()),
        meta("apply_patch", "Apply a unified diff patch with AST validation", ToolCategory::FileSystem, local_fs()),
        meta("grep_files", "Search file contents with ripgrep (falls back to grep)", ToolCategory::FileSystem, read_fs()),
        meta("find_definition", "Find symbol definitions using tree-sitter AST parsing", ToolCategory::FileSystem, read_fs()),
        meta("repomap", "Generate a structural map of the repository", ToolCategory::FileSystem, read_fs()),
        meta("workspace_perception", "Scan workspace structure and detect project type", ToolCategory::FileSystem, read_fs()),
        meta("custom_scout", "Deep recursive file search with pattern matching", ToolCategory::FileSystem, read_fs()),
        // --- Git ---
        meta("git_status", "Show working tree status with structured summary", ToolCategory::FileSystem, read_fs()),
        meta("git_diff", "Show detailed diffs (staged/unstaged, per-file, stat)", ToolCategory::FileSystem, read_fs()),
        meta("git_add", "Stage specific files for commit (safe, no auto-add-all)", ToolCategory::FileSystem, local_fs()),
        meta("git_commit", "Commit staged changes with message", ToolCategory::FileSystem, local_fs()),
        meta("git_log", "Show commit history with filtering options", ToolCategory::FileSystem, read_fs()),
        meta("git_branch", "List, create, or delete git branches", ToolCategory::FileSystem, local_fs()),
        // --- Network ---
        meta("browser", "Open and interact with web pages in a headless browser", ToolCategory::Network, network()),
        meta("web_fetch", "Fetch URL and convert HTML to readable Markdown text", ToolCategory::Network, network()),
        meta("brave_search", "Search the web using Brave Search API", ToolCategory::Network, network()),
        meta("google_search", "Search the web using Google Search", ToolCategory::Network, network()),
        meta("search", "Search the web (auto-routes to best free backend)", ToolCategory::Network, network()),
        meta("duckduckgo_search", "Search the web using DuckDuckGo (free, no API key needed)", ToolCategory::Network, network()),
        meta("link_understanding", "Analyze and extract structured data from a URL", ToolCategory::Network, network()),
        // --- System ---
        meta("shell", "Execute a shell command and return output (60s timeout)", ToolCategory::System, dangerous_shell()),
        meta("terminal", "Persistent shell session with state preservation (cd, env)", ToolCategory::System, dangerous_shell()),
        meta("run_background", "Start a command in the background with interactive I/O", ToolCategory::System, dangerous_shell()),
        meta("get_process_status", "Check the status and read output of a background process", ToolCategory::System, local_fs()),
        meta("send_command_input", "Send stdin input to a running background process", ToolCategory::System, dangerous_shell()),
        meta("checkpoint", "Save a checkpoint of current agent state", ToolCategory::System, local_fs()),
        // --- Coding ---
        meta("lsp", "Query Language Server Protocol for code intelligence", ToolCategory::Coding, pure()),
        // --- Autonomous ---
        meta("regret_minimization", "Store and query failed approaches to avoid repeating mistakes", ToolCategory::System, local_fs()),
        meta("reflect", "Perform deep self-reflection on recent actions and outcomes", ToolCategory::Autonomous, pure()),
        meta("failure_prediction", "Predict failure risk based on complexity and historical fix frequency", ToolCategory::Core, pure()),
        meta("proactive_self_optimization", "Scan codebase for performance bottlenecks and smells", ToolCategory::System, local_fs()),
        meta("hydrate_context", "Load relevant context from knowledge base into working memory", ToolCategory::Autonomous, pure()),
        meta("memory_management", "Manage long-term memory entries", ToolCategory::Autonomous, local_fs()),
        meta("ingest_ast_knowledge", "Parse source code AST and index into knowledge graph", ToolCategory::Autonomous, local_fs()),
        // --- Orchestration ---
        meta("fractal_swarm", "Delegate tasks to parallel sub-agents or threads", ToolCategory::Orchestration, dangerous_shell()),
        meta("subagent", "Spawn a specialized sub-agent for a focused task", ToolCategory::Orchestration, autonomous()),
        meta("ephemeral_red_team", "Run temporary adversarial tests and auto-cleanup", ToolCategory::System, dangerous_shell()),
        meta("pharmakon_task", "Delegate tasks recursively to an independent Pharmakon sub-instance", ToolCategory::Orchestration, autonomous()),
        // --- Observability ---
        meta("temporal_awareness", "Analyze git history, code churn, and author contributions", ToolCategory::System, local_fs()),
        // --- Media ---
        meta("screenshot", "Capture a screenshot of the current screen", ToolCategory::Media, pure()),
        meta("camera", "Capture image from camera", ToolCategory::Media, pure()),
        meta("generate_image", "Generate beautiful UI mockups, placeholders, or DALL-E assets and save locally", ToolCategory::Media, dangerous_net()),
        meta("media_understanding", "Analyze images and media using vision model", ToolCategory::Media, pure()),
        meta("canvas", "Draw interactive visualizations on a shared canvas", ToolCategory::Media, local_fs()),
        // --- Misc ---
        meta("task_tracker", "Track and manage project tasks and milestones", ToolCategory::Core, local_fs()),
        meta("commitment", "Record and track commitments and promises", ToolCategory::Core, local_fs()),
        meta("context_connector", "Connect to external knowledge sources", ToolCategory::Core, pure()),
        meta("soul_manager", "Modify agent personality and behavioral traits", ToolCategory::Core, local_fs()),
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
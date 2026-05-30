use pharmakon_common::AgentError;
use serde::{Serialize, de::DeserializeOwned};
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

pub fn state_dir(name: &str) -> Result<PathBuf, AgentError> {
    let base = dirs::home_dir()
        .ok_or_else(|| AgentError("Could not find home directory".to_string()))?
        .join(".pharmakon")
        .join(name);
    fs::create_dir_all(&base)
        .map_err(|e| AgentError(format!("Failed to create state dir: {}", e)))?;
    Ok(base)
}

pub fn read_json<T: DeserializeOwned + Default>(path: &Path) -> Result<T, AgentError> {
    if !path.exists() {
        return Ok(T::default());
    }
    let data = fs::read_to_string(path)
        .map_err(|e| AgentError(format!("Failed to read {}: {}", path.display(), e)))?;
    serde_json::from_str(&data)
        .map_err(|e| AgentError(format!("Failed to parse {}: {}", path.display(), e)))
}

pub fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), AgentError> {
    let data = serde_json::to_string_pretty(value)
        .map_err(|e| AgentError(format!("Failed to serialize json: {}", e)))?;
    fs::write(path, data)
        .map_err(|e| AgentError(format!("Failed to write {}: {}", path.display(), e)))
}

pub fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}

pub fn short_hash(input: &str) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    input.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

pub fn estimate_tokens(text: &str) -> usize {
    (text.chars().count() / 4).max(1)
}

pub fn tokenize(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        .filter(|s| s.len() > 1)
        .map(|s| s.to_ascii_lowercase())
        .collect()
}

pub fn is_probably_binary(bytes: &[u8]) -> bool {
    bytes.iter().take(1024).any(|b| *b == 0)
}

pub fn metadata_modified_secs(path: &Path) -> u64 {
    path.metadata()
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or_default()
}

pub fn scan_diff_risks(text: &str) -> Vec<String> {
    let mut risks = Vec::new();
    let lower = text.to_ascii_lowercase();
    let secret_markers = [
        "api_key",
        "apikey",
        "secret",
        "private_key",
        "access_token",
        "bearer ",
        "password",
        "BEGIN RSA PRIVATE KEY",
        "BEGIN OPENSSH PRIVATE KEY",
    ];
    for marker in secret_markers {
        if lower.contains(&marker.to_ascii_lowercase()) {
            risks.push(format!(
                "Possible secret material or credential marker: {}",
                marker
            ));
        }
    }
    if lower.contains("chmod 777") {
        risks.push("World-writable permission change detected".to_string());
    }
    if lower.contains("rm -rf /") || lower.contains("rm -rf *") {
        risks.push("Dangerous recursive removal command detected".to_string());
    }
    if lower.contains("unsafe {") {
        risks.push("Rust unsafe block introduced".to_string());
    }
    if lower.contains("select ") && lower.contains("format!(") {
        risks.push("Possible SQL construction through string formatting".to_string());
    }
    if lower.contains("std::env::set_var") {
        risks.push("Process environment mutation detected".to_string());
    }
    risks
}

pub fn find_rust_function_span(content: &str, name: &str) -> Option<(usize, usize, usize, usize)> {
    let needle = format!("fn {}", name);
    let fn_pos = content.find(&needle)?;
    let brace_start = content[fn_pos..].find('{')? + fn_pos;
    let bytes = content.as_bytes();
    let mut depth = 0i32;
    let mut end = None;
    for (i, byte) in bytes.iter().enumerate().skip(brace_start) {
        match byte {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    end = Some(i + 1);
                    break;
                }
            }
            _ => {}
        }
    }
    end.map(|e| (fn_pos, brace_start, brace_start + 1, e))
}

#[macro_export]
macro_rules! planning_tool {
    ($struct_name:ident, $tool_name:expr, $desc:expr, $category:expr) => {
        pub struct $struct_name;

        #[async_trait]
        impl Tool for $struct_name {
            fn name(&self) -> &str { $tool_name }
            fn description(&self) -> &str { $desc }
            fn parameters(&self) -> Value {
                json!({
                    "type": "object",
                    "properties": {
                        "goal": { "type": "string" },
                        "options": { "type": "array" },
                        "items": { "type": "array" },
                        "context": { "type": "string" },
                        "top_k": { "type": "integer", "default": 5 }
                    }
                })
            }
            fn category(&self) -> ToolCategory { $category }
            async fn call(&self, args: Value) -> AgentResult<String> {
                let goal = args["goal"].as_str().unwrap_or("unspecified");
                let top_k = args["top_k"].as_u64().unwrap_or(5) as usize;
                let options = args["options"].as_array().cloned().unwrap_or_default();
                let mut ranked = Vec::new();
                for (idx, option) in options.iter().enumerate() {
                    let text = option.as_str().map(|s| s.to_string()).unwrap_or_else(|| option.to_string());
                    let risk = $crate::codex_utils::scan_diff_risks(&text).len() as f64;
                    ranked.push(json!({
                        "option": option,
                        "score": (1.0 / ((idx + 1) as f64)) - (risk * 0.2),
                        "risk_signals": risk
                    }));
                }
                ranked.truncate(top_k);
                Ok(json!({
                    "tool": $tool_name,
                    "goal": goal,
                    "status": "analysis_ready",
                    "ranked": ranked,
                    "next_step": "Use the ranked output as decision support; this tool does not mutate workspace state."
                }).to_string())
            }
        }
    };
}

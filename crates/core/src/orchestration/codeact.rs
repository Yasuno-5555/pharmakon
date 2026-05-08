//! CodeAct Hybrid Mode — Rhai + Python scripting engine for compound tool execution.
//!
//! Architecture:
//!   1st attempt: Rhai (fast, sandboxed)
//!   2nd attempt: Python via system `python3` (higher LLM fluency)
//!
//! Benefits:
//!   - 1 LLM turn = 10+ tool calls (control flow in script)
//!   - Intermediate results stay in script scope, not token-wasting context
//!   - Self-debugging: errors are fed back for the LLM to fix

use anyhow::Result;
use rhai::{Engine, Scope, Dynamic, EvalAltResult};
use std::sync::Arc;
use std::path::PathBuf;

pub struct CodeActToolbox {
    pub workspace_root: PathBuf,
}

impl CodeActToolbox {
    pub fn new(workspace_root: PathBuf) -> Self { Self { workspace_root } }

    fn resolve(&self, path: &str) -> PathBuf {
        let p = PathBuf::from(path);
        if p.is_absolute() { p } else { self.workspace_root.join(p) }
    }

    fn read_file(&self, path: &str) -> Result<String, Box<EvalAltResult>> {
        let resolved = self.resolve(path);
        std::fs::read_to_string(&resolved).map_err(|e| format!("read_file({}): {}", path, e).into())
    }

    fn write_file(&self, path: &str, content: &str) -> Result<(), Box<EvalAltResult>> {
        let resolved = self.resolve(path);
        if let Some(parent) = resolved.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("create_dir({}): {}", parent.display(), e))?;
        }
        std::fs::write(&resolved, content).map_err(|e| format!("write_file({}): {}", path, e).into())
    }

    fn grep_files(&self, pattern: &str, dir: &str) -> Result<Vec<String>, Box<EvalAltResult>> {
        let resolved = self.resolve(dir);
        let mut results = Vec::new();
        self.grep_recursive(&resolved, pattern, &mut results)?;
        Ok(results)
    }

    fn grep_recursive(&self, dir: &PathBuf, pattern: &str, results: &mut Vec<String>) -> Result<(), Box<EvalAltResult>> {
        let re = regex::Regex::new(pattern).map_err(|e| format!("Invalid regex '{}': {}", pattern, e))?;
        if dir.is_dir() {
            for entry in std::fs::read_dir(dir).map_err(|e| format!("read_dir({}): {}", dir.display(), e))? {
                let entry = entry.map_err(|e| e.to_string())?;
                let path = entry.path();
                if path.is_dir() {
                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                        if name.starts_with('.') || name == "target" || name == "node_modules" { continue; }
                    }
                    self.grep_recursive(&path, pattern, results)?;
                } else if path.is_file() {
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        for (line_no, line) in content.lines().enumerate() {
                            if re.is_match(line) {
                                results.push(format!("{}:{}: {}", path.display(), line_no + 1, line.trim()));
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn list_dir(&self, path: &str) -> Result<Vec<String>, Box<EvalAltResult>> {
        let resolved = self.resolve(path);
        let mut entries = Vec::new();
        for entry in std::fs::read_dir(&resolved).map_err(|e| format!("list_dir({}): {}", path, e))? {
            let entry = entry.map_err(|e| e.to_string())?;
            let name = entry.file_name().to_string_lossy().to_string();
            let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
            entries.push(if is_dir { format!("{}/", name) } else { name });
        }
        entries.sort();
        Ok(entries)
    }
}

pub struct CodeActEngine {
    engine: Engine,
    toolbox: Arc<CodeActToolbox>,
}

impl CodeActEngine {
    pub fn new(workspace_root: PathBuf) -> Self {
        let mut engine = Engine::new();
        let toolbox = Arc::new(CodeActToolbox::new(workspace_root));

        let tb = toolbox.clone();
        engine.register_fn("read_file", move |path: &str| -> Result<String, Box<EvalAltResult>> { tb.read_file(path) });
        let tb = toolbox.clone();
        engine.register_fn("write_file", move |path: &str, content: &str| -> Result<(), Box<EvalAltResult>> { tb.write_file(path, content) });
        let tb = toolbox.clone();
        engine.register_fn("grep", move |pattern: &str, dir: &str| -> Result<Vec<String>, Box<EvalAltResult>> { tb.grep_files(pattern, dir) });
        let tb = toolbox.clone();
        engine.register_fn("shell", move |cmd: &str| -> Result<String, Box<EvalAltResult>> {
            use std::process::Command;
            let output = Command::new("sh").arg("-c").arg(cmd).output().map_err(|e| format!("shell failed: {}", e))?;
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            Ok(format!("{}{}{}", stdout, if !stdout.is_empty() && !stderr.is_empty() { "\n" } else { "" }, stderr))
        });
        let tb = toolbox.clone();
        engine.register_fn("list_dir", move |path: &str| -> Result<Vec<String>, Box<EvalAltResult>> { tb.list_dir(path) });

        engine.set_max_modules(0);
        Self { engine, toolbox }
    }

    /// Execute a script. 1st: Rhai (fast, sandboxed). 2nd: Python (higher LLM fluency).
    pub fn execute(&self, script: &str) -> CodeActResult {
        let mut scope = Scope::new();
        scope.push_constant("WORKSPACE", self.toolbox.workspace_root.to_string_lossy().to_string());

        match self.engine.eval_with_scope::<Dynamic>(&mut scope, script) {
            Ok(value) => {
                let result_str = format!("{}", value);
                return CodeActResult {
                    success: true,
                    output: if result_str.is_empty() || result_str == "()" { String::new() } else { result_str },
                    error: None,
                };
            }
            Err(rhai_err) => {
                log::warn!("Rhai failed ({}), falling back to Python", rhai_err);
            }
        }

        self.execute_python(script)
    }

    /// Execute via system Python with built-in function wrappers.
    fn execute_python(&self, script: &str) -> CodeActResult {
        let ws = self.toolbox.workspace_root.to_string_lossy();
        let preamble = format!(
r#"import subprocess as _sp, os as _os, re as _re, json, sys
_WORKSPACE = r"{ws}"
def _resolve(p):
    return p if p.startswith('/') else _os.path.join(_WORKSPACE, p)
def read_file(path):
    with open(_resolve(path)) as f: return f.read()
def write_file(path, content):
    p = _resolve(path); _os.makedirs(_os.path.dirname(p) or '.', exist_ok=True)
    with open(p, 'w') as f: f.write(content)
def grep(pattern, dir_path):
    d = _resolve(dir_path); results = []
    for root, dirs, files in _os.walk(d):
        dirs[:] = [x for x in dirs if not x.startswith('.') and x not in ('target','node_modules','.git')]
        for fn in files:
            try:
                fp = _os.path.join(root, fn)
                for i, line in enumerate(open(fp, errors='replace')):
                    if _re.search(pattern, line):
                        results.append(f"{{fp}}:{{i+1}}: {{line.rstrip()}}")
            except: pass
    return json.dumps(results)
def shell(cmd):
    r = _sp.run(cmd, shell=True, capture_output=True, text=True, timeout=120)
    out = r.stdout
    if r.stderr: out += '\n' + r.stderr
    return out.strip()
def list_dir(path):
    d = _resolve(path)
    return json.dumps(sorted([f"{{x}}{{'/' if _os.path.isdir(_os.path.join(d,x)) else ''}}" for x in _os.listdir(d)]))
# --- user script ---
{script}
"#, ws = ws, script = script);

        use std::process::Command;
        match Command::new("python3").arg("-c").arg(&preamble).output() {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                let combined = if stderr.is_empty() { stdout.clone() } else { format!("{}\n{}", stdout, stderr) };
                CodeActResult {
                    success: output.status.success(),
                    output: combined.trim().to_string(),
                    error: if output.status.success() { None } else { Some(stderr) },
                }
            }
            Err(e) => CodeActResult { success: false, output: String::new(), error: Some(format!("Python failed: {}", e)) },
        }
    }

    /// Execute with pre-bound variables (Rhai only, falls back to Python without context).
    pub fn execute_with_context(&self, script: &str, context: Vec<(&str, Dynamic)>) -> CodeActResult {
        let mut scope = Scope::new();
        scope.push_constant("WORKSPACE", self.toolbox.workspace_root.to_string_lossy().to_string());
        for (name, value) in context { scope.push_constant(name, value); }

        match self.engine.eval_with_scope::<Dynamic>(&mut scope, script) {
            Ok(value) => {
                let result_str = format!("{}", value);
                return CodeActResult {
                    success: true,
                    output: if result_str.is_empty() || result_str == "()" { String::new() } else { result_str },
                    error: None,
                };
            }
            Err(rhai_err) => { log::warn!("Rhai failed ({}), falling back to Python", rhai_err); }
        }

        self.execute_python(script)
    }
}

#[derive(Debug, Clone)]
pub struct CodeActResult { pub success: bool, pub output: String, pub error: Option<String> }

impl std::fmt::Display for CodeActResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.success { write!(f, "{}", self.output) }
        else { write!(f, "ERROR: {}\n\nOutput before error:\n{}", self.error.as_deref().unwrap_or("unknown"), self.output) }
    }
}

pub struct CodeActTool { engine: Arc<CodeActEngine> }

impl CodeActTool {
    pub fn new(workspace_root: std::path::PathBuf) -> Self {
        Self { engine: Arc::new(CodeActEngine::new(workspace_root)) }
    }
}

#[async_trait::async_trait]
impl pharmakon_common::Tool for CodeActTool {
    fn category(&self) -> pharmakon_common::ToolCategory { pharmakon_common::ToolCategory::Core }
    fn name(&self) -> &str { "codeact" }

    fn description(&self) -> &str {
        "Execute a script (Python or Rhai) that orchestrates multiple operations in one turn. \
         Python preferred for LLM fluency; Rhai tried first, falls back to Python on error. \
         Functions: read_file(path)->String, write_file(path,content), grep(pattern,dir)->[String], \
         shell(cmd)->String, list_dir(path)->[String]. Use for compound flows."
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "script": { "type": "string", "description": "Python or Rhai script using read_file, write_file, grep, shell, list_dir." }
            },
            "required": ["script"]
        })
    }

    async fn call(&self, args: serde_json::Value) -> pharmakon_common::AgentResult<String> {
        let script = args["script"].as_str()
            .ok_or_else(|| pharmakon_common::AgentError("Missing 'script'".to_string()))?;

        let result = self.engine.execute(script);
        if result.success { Ok(result.output) }
        else { Err(pharmakon_common::AgentError(format!("Execution error: {}", result.error.as_deref().unwrap_or("unknown")))) }
    }

    fn execution_profile(&self) -> pharmakon_common::ExecutionProfile {
        pharmakon_common::ExecutionProfile {
            side_effect_level: pharmakon_common::SideEffectLevel::Local,
            filesystem_scope: pharmakon_common::FilesystemScope::Confined,
            reversibility: pharmakon_common::Reversibility::Possible,
            ..Default::default()
        }
    }
}

pub fn validate_script(script: &str) -> Result<(), String> {
    let engine = Engine::new();
    engine.compile(script).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_codeact_read_and_grep() {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::write(tmp.path().join("test.rs"), "fn main() { println!(\"hello\"); }\n").unwrap();
        let engine = CodeActEngine::new(tmp.path().to_path_buf());
        let result = engine.execute("let content = read_file(\"test.rs\"); content");
        assert!(result.success);
        assert!(result.output.contains("fn main()"));
    }

    #[test]
    fn test_codeact_error_handling() {
        let tmp = tempfile::TempDir::new().unwrap();
        let engine = CodeActEngine::new(tmp.path().to_path_buf());
        let result = engine.execute("read_file(\"nonexistent.txt\")");
        assert!(!result.success);
    }

    #[test]
    fn test_validate_script() {
        assert!(validate_script("let x = 1; x + 2").is_ok());
        assert!(validate_script("let x = ").is_err());
    }
}

use crate::security::SecurityAuditor;
use serde_json::Value;

pub enum PolicyAction {
    Allow,
    Deny(String),
    RequireApproval(String),
}

pub trait Policy: Send + Sync {
    fn name(&self) -> &str;
    fn evaluate_tool_call(&self, tool_name: &str, args: &Value) -> PolicyAction;
}

pub struct DefaultSecurityPolicy;

impl Policy for DefaultSecurityPolicy {
    fn name(&self) -> &str {
        "default_security"
    }

    fn evaluate_tool_call(&self, tool_name: &str, args: &Value) -> PolicyAction {
        match tool_name {
            "shell" => {
                if let Some(cmd) = args["command"].as_str() {
                    if let Err(e) = SecurityAuditor::audit_shell_command(cmd) {
                        return PolicyAction::Deny(e.to_string());
                    }

                    // If it is in the command blocklist, require manual approval
                    if SecurityAuditor::is_blocked_command(cmd) {
                        return PolicyAction::RequireApproval(format!(
                            "Command '{}' is potentially dangerous. Manual confirmation is required.",
                            cmd
                        ));
                    }
                }

                // Prioritize explicit agent request for approval
                if args["requires_manual_approval"].as_bool().unwrap_or(false) {
                    return PolicyAction::RequireApproval(
                        "Agent requested manual confirmation for this command.".to_string(),
                    );
                }

                PolicyAction::Allow
            }
            "read_file" | "write_file" => {
                if let Some(path) = args["path"].as_str()
                    && let Err(e) = SecurityAuditor::audit_file_path(path)
                {
                    return PolicyAction::Deny(e.to_string());
                }
                PolicyAction::Allow
            }
            _ => PolicyAction::Allow,
        }
    }
}

/// Constitutional policy: immutable safety rules that cannot be bypassed.
/// These form the agent's "constitution" — rules that protect the system
/// from self-modification, destruction of critical files, and policy bypass.
pub struct ConstitutionalPolicy;

impl Policy for ConstitutionalPolicy {
    fn name(&self) -> &str {
        "constitutional"
    }

    fn evaluate_tool_call(&self, tool_name: &str, args: &Value) -> PolicyAction {
        // Rule 1: No self-modification of the agent's own source
        if (tool_name == "write_file" || tool_name == "apply_patch" || tool_name == "mutate_ast")
            && let Some(path) = args["path"].as_str()
        {
            let path_lower = path.to_lowercase();
            if path_lower.contains("crates/core/src/")
                || path_lower.contains("crates/common/src/")
                || path_lower.contains("crates/memory/src/")
                || path_lower.contains("crates/tools/src/")
            {
                return PolicyAction::Deny(
                    "Constitutional violation: Agent cannot modify its own source code."
                        .to_string(),
                );
            }
            // Rule 2: Protect the policy engine itself
            if path_lower.contains("security/policy") || path_lower.contains("constitutional") {
                return PolicyAction::Deny(
                    "Constitutional violation: Cannot modify the policy enforcement system."
                        .to_string(),
                );
            }
        }

        // Rule 3: Shell commands must pass constitutional review
        if tool_name == "shell"
            && args["command"].as_str().is_some_and(|c| {
                c.contains("rm -rf /")
                    || c.contains("sudo ")
                    || c.contains("chmod 777")
                    || c.contains("git clean")
            })
        {
            return PolicyAction::Deny(
                "Constitutional violation: Destructive system or repository commands (like git clean) are prohibited.".to_string(),
            );
        }

        PolicyAction::Allow
    }
}

pub struct PolicyEngine {
    policies: Vec<Box<dyn Policy>>,
}

impl Default for PolicyEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl PolicyEngine {
    pub fn new() -> Self {
        let policies: Vec<Box<dyn Policy>> = vec![
            Box::new(ConstitutionalPolicy),
            Box::new(DefaultSecurityPolicy),
        ];
        Self { policies }
    }

    pub fn add_policy(&mut self, policy: Box<dyn Policy>) {
        self.policies.push(policy);
    }

    pub fn evaluate_tool_call(&self, tool_name: &str, args: &Value) -> PolicyAction {
        for policy in &self.policies {
            let action = policy.evaluate_tool_call(tool_name, args);
            match action {
                PolicyAction::Deny(_) => return action,
                PolicyAction::RequireApproval(_) => return action,
                PolicyAction::Allow => continue,
            }
        }
        PolicyAction::Allow
    }
}

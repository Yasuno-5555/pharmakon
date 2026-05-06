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
                if let Some(path) = args["path"].as_str() {
                    if let Err(e) = SecurityAuditor::audit_file_path(path) {
                        return PolicyAction::Deny(e.to_string());
                    }
                }
                PolicyAction::Allow
            }
            _ => PolicyAction::Allow,
        }
    }
}

pub struct PolicyEngine {
    policies: Vec<Box<dyn Policy>>,
}

impl PolicyEngine {
    pub fn new() -> Self {
        Self {
            policies: vec![Box::new(DefaultSecurityPolicy)],
        }
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

pub mod redaction;
pub mod policy;
pub mod pairing;
use anyhow::{Result, anyhow};
use std::path::Path;

pub struct SecurityAuditor;

impl SecurityAuditor {
    pub fn audit_shell_command(command: &str) -> Result<()> {
        let risky_patterns = [
            "rm -rf /",
            "mv /",
            "> /dev/sda",
            ":(){ :|:& };:", // Fork bomb
            "mkfs",
            "dd if=",
        ];

        for pattern in risky_patterns {
            if command.contains(pattern) {
                return Err(anyhow!("Risky command pattern detected: '{}'", pattern));
            }
        }

        Ok(())
    }

    pub fn is_allowed_command(command: &str) -> bool {
        let allowlist = [
            "ls",
            "pwd",
            "whoami",
            "date",
            "cat ",
            "grep ",
        ];

        for pattern in allowlist {
            if command.starts_with(pattern) {
                return true;
            }
        }
        false
    }

    pub fn audit_file_path(path_str: &str) -> Result<()> {
        let path = Path::new(path_str);
        
        // Prevent access to sensitive system directories
        let sensitive_dirs = [
            "/etc",
            "/var/root",
            "/System",
            "/usr/bin",
        ];

        for dir in sensitive_dirs {
            if path.starts_with(dir) {
                return Err(anyhow!("Access to sensitive system directory denied: '{}'", dir));
            }
        }

        // Prevent path traversal
        if path_str.contains("..") {
            return Err(anyhow!("Path traversal pattern detected"));
        }

        Ok(())
    }
}

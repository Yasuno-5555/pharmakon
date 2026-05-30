pub mod pairing;
pub mod policy;
pub mod redaction;
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

    pub fn is_blocked_command(command: &str) -> bool {
        let trimmed = command.trim_start();
        let blocklist = [
            "rm ",       // file deletion (single files, recursive)
            "curl ",     // arbitrary network requests
            "wget ",     // arbitrary network requests
            "sudo ",     // privilege escalation
            "chmod ",    // permission changes
            "chown ",    // ownership changes
            "dd ",       // low-level disk I/O
            "kill ",     // process termination
            "pkill ",    // process termination by name
            "reboot",    // system restart
            "shutdown",  // system shutdown
            "halt",      // system halt
            "poweroff",  // system power off
            "systemctl", // service management
            "launchctl", // macOS service management
        ];

        for pattern in blocklist {
            if trimmed.starts_with(pattern) {
                return true;
            }
        }
        false
    }

    pub fn audit_file_path(path_str: &str) -> Result<()> {
        let path = Path::new(path_str);

        // Prevent access to sensitive system directories
        let sensitive_dirs = ["/etc", "/var/root", "/System", "/usr/bin"];

        for dir in sensitive_dirs {
            if path.starts_with(dir) {
                return Err(anyhow!(
                    "Access to sensitive system directory denied: '{}'",
                    dir
                ));
            }
        }

        // Prevent path traversal
        for component in path.components() {
            if component == std::path::Component::ParentDir {
                return Err(anyhow!("Path traversal pattern detected"));
            }
        }

        Ok(())
    }
}

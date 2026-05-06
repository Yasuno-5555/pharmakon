use anyhow::{Result, anyhow};
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

pub fn install_service(port: u16) -> Result<()> {
    let os = env::consts::OS;
    let exe_path = env::current_exe()?;
    let home = dirs::home_dir().ok_or_else(|| anyhow!("Could not find home directory"))?;

    match os {
        "macos" => install_macos_launchd(exe_path, home, port),
        "linux" => install_linux_systemd(exe_path, home, port),
        _ => Err(anyhow!("Service installation not supported on {}", os)),
    }
}

fn install_macos_launchd(exe_path: PathBuf, home: PathBuf, port: u16) -> Result<()> {
    let plist_dir = home.join("Library").join("LaunchAgents");
    fs::create_dir_all(&plist_dir)?;

    let plist_path = plist_dir.join("ai.openclaw.pharmakon.plist");
    let label = "ai.openclaw.pharmakon";

    let plist_content = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{}</string>
        <string>gateway</string>
        <string>--port</string>
        <string>{}</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>StandardOutPath</key>
    <string>{}/.pharmakon/logs/gateway.out</string>
    <key>StandardErrorPath</key>
    <string>{}/.pharmakon/logs/gateway.err</string>
</dict>
</plist>"#,
        label,
        exe_path.to_str().unwrap(),
        port,
        home.to_str().unwrap(),
        home.to_str().unwrap()
    );

    fs::write(&plist_path, plist_content)?;

    println!("✅ Launchd plist created at {:?}", plist_path);
    println!("To start the service, run:");
    println!("  launchctl load {:?}", plist_path);

    Ok(())
}

fn install_linux_systemd(exe_path: PathBuf, home: PathBuf, port: u16) -> Result<()> {
    let systemd_dir = home.join(".config").join("systemd").join("user");
    fs::create_dir_all(&systemd_dir)?;

    let service_path = systemd_dir.join("pharmakon.service");

    let service_content = format!(
        r#"[Unit]
Description=Pharmakon Personal AI Assistant Gateway
After=network.target

[Service]
ExecStart={} gateway --port {}
Restart=always
RestartSec=10
StandardOutput=append:{}/.pharmakon/logs/gateway.out
StandardError=append:{}/.pharmakon/logs/gateway.err

[Install]
WantedBy=default.target"#,
        exe_path.to_str().unwrap(),
        port,
        home.to_str().unwrap(),
        home.to_str().unwrap()
    );

    fs::write(&service_path, service_content)?;

    println!("✅ Systemd service created at {:?}", service_path);
    println!("To start the service, run:");
    println!("  systemctl --user daemon-reload");
    println!("  systemctl --user enable --now pharmakon");

    Ok(())
}

pub fn stop_service() -> Result<()> {
    let os = env::consts::OS;
    match os {
        "macos" => {
            let home = dirs::home_dir().ok_or_else(|| anyhow!("Could not find home directory"))?;
            let plist_path = home
                .join("Library")
                .join("LaunchAgents")
                .join("ai.openclaw.pharmakon.plist");

            if plist_path.exists() {
                let _ = Command::new("launchctl")
                    .args(&["unload", plist_path.to_str().unwrap()])
                    .status();
            }

            // Force cleanup: kill any process listening on standard pharmakon ports
            let ports = vec!["18789", "19999"];
            for port in ports {
                if let Ok(output) = Command::new("lsof")
                    .args(["-ti", &format!(":{}", port)])
                    .output()
                {
                    let pids = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    if !pids.is_empty() {
                        for pid in pids.lines() {
                            let _ = Command::new("kill").arg("-9").arg(pid).output();
                        }
                    }
                }
            }
            println!("✅ Service stopped (unloaded from launchd).");
            Ok(())
        }
        "linux" => {
            let _ = Command::new("systemctl")
                .args(&["--user", "stop", "pharmakon"])
                .status();

            // Force cleanup: kill any process listening on standard pharmakon ports
            let ports = vec!["18789", "19999"];
            for port in ports {
                if let Ok(output) = Command::new("fuser")
                    .args(["-k", &format!("{}/tcp", port)])
                    .output()
                {
                    let _ = output;
                }
            }
            println!("✅ Service stopped (systemd).");
            Ok(())
        }
        _ => Err(anyhow!("Service management not supported on {}", os)),
    }
}

pub fn get_service_status() -> Result<()> {
    let os = env::consts::OS;
    match os {
        "macos" => {
            let label = "ai.openclaw.pharmakon";
            let output = std::process::Command::new("launchctl")
                .args(&["list", label])
                .output()?;

            if output.status.success() {
                println!("🟢 Service is running (launchd).");
            } else {
                println!("🔴 Service is not running or not loaded.");
            }
            Ok(())
        }
        "linux" => {
            let output = std::process::Command::new("systemctl")
                .args(&["--user", "is-active", "pharmakon"])
                .output()?;
            let status = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if status == "active" {
                println!("🟢 Service is active (systemd).");
            } else {
                println!("🔴 Service status: {}", status);
            }
            Ok(())
        }
        _ => Err(anyhow!("Service management not supported on {}", os)),
    }
}

use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use tracing::info;

/// Service manager for systemd (Linux) and launchd (macOS)
#[derive(Debug, Clone)]
pub struct ServiceManager;

impl ServiceManager {
    /// Install systemd user service on Linux
    #[cfg(target_os = "linux")]
    pub async fn install_systemd() -> Result<()> {
        info!("Installing 9Router systemd user service...");

        let home_dir = dirs::home_dir().context("Could not determine home directory")?;
        let systemd_dir = home_dir.join(".config").join("systemd").join("user");
        let service_file = systemd_dir.join("9router-bootstrap.service");

        // Create systemd user directory if it doesn't exist
        fs::create_dir_all(&systemd_dir).context("Failed to create systemd user directory")?;

        // Get the path to the bootstrap binary
        let bootstrap_binary = std::env::current_exe()
            .context("Failed to get current executable path")?
            .to_string_lossy()
            .to_string();

        // Find the 9router server script
        let server_script = Self::find_server_script()?;

        info!("Bootstrap binary: {}", bootstrap_binary);
        info!("Server script: {}", server_script);

        // Create systemd service file
        let service_content = format!(
            r#"[Unit]
Description=9Router Bootstrap Service
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
ExecStart={} start {}
Restart=always
RestartSec=10
Environment=NODE_ENV=production
Environment=NINEROUTER_BOOTSTRAP=1

# Security hardening
NoNewPrivileges=true
ProtectSystem=full
ProtectHome=read-only
ReadWritePaths=%h/.9router
PrivateTmp=true

[Install]
WantedBy=default.target
"#,
            bootstrap_binary, server_script
        );

        fs::write(&service_file, service_content)
            .context("Failed to write systemd service file")?;

        info!("Service file written to: {}", service_file.display());

        // Reload systemd daemon
        info!("Reloading systemd daemon...");
        Command::new("systemctl")
            .args(["--user", "daemon-reload"])
            .output()
            .context("Failed to reload systemd daemon")?;

        // Enable and start the service
        info!("Enabling and starting service...");
        Command::new("systemctl")
            .args(["--user", "enable", "--now", "9router-bootstrap.service"])
            .output()
            .context("Failed to enable and start service")?;

        info!("✅ 9Router bootstrap service installed and started");
        info!("📊 Check status: systemctl --user status 9router-bootstrap.service");
        info!("📋 View logs: journalctl --user -u 9router-bootstrap.service -f");

        Ok(())
    }

    /// Uninstall systemd user service
    #[cfg(target_os = "linux")]
    pub async fn uninstall_systemd() -> Result<()> {
        info!("Uninstalling 9Router systemd user service...");

        let home_dir = dirs::home_dir().context("Could not determine home directory")?;
        let systemd_dir = home_dir.join(".config").join("systemd").join("user");
        let service_file = systemd_dir.join("9router-bootstrap.service");

        // Stop and disable the service
        info!("Stopping service...");
        let _ = Command::new("systemctl")
            .args(["--user", "stop", "9router-bootstrap.service"])
            .output();

        info!("Disabling service...");
        let _ = Command::new("systemctl")
            .args(["--user", "disable", "9router-bootstrap.service"])
            .output();

        // Remove service file
        if service_file.exists() {
            fs::remove_file(&service_file).context("Failed to remove service file")?;
            info!("Service file removed: {}", service_file.display());
        }

        // Reload systemd daemon
        Command::new("systemctl")
            .args(["--user", "daemon-reload"])
            .output()
            .context("Failed to reload systemd daemon")?;

        info!("✅ 9Router bootstrap service uninstalled");
        Ok(())
    }

    /// Install launchd agent on macOS
    #[cfg(target_os = "macos")]
    pub async fn install_launchd() -> Result<()> {
        info!("Installing 9Router launchd agent...");

        let home_dir = dirs::home_dir().context("Could not determine home directory")?;
        let launch_agents_dir = home_dir.join("Library").join("LaunchAgents");
        let plist_file = launch_agents_dir.join("com.9router.bootstrap.plist");

        // Create LaunchAgents directory if it doesn't exist
        fs::create_dir_all(&launch_agents_dir).context("Failed to create LaunchAgents directory")?;

        // Get the path to the bootstrap binary
        let bootstrap_binary = std::env::current_exe()
            .context("Failed to get current executable path")?
            .to_string_lossy()
            .to_string();

        let server_script = Self::find_server_script()?;

        // Create launchd plist
        let plist_content = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.9router.bootstrap</string>
    <key>ProgramArguments</key>
    <array>
        <string>{}</string>
        <string>start</string>
        <string>{}</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>StandardOutPath</key>
    <string>{}/Library/Logs/9router-bootstrap.log</string>
    <key>StandardErrorPath</key>
    <string>{}/Library/Logs/9router-bootstrap-error.log</string>
    <key>EnvironmentVariables</key>
    <dict>
        <key>PATH</key>
        <string>/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin</string>
        <key>NODE_ENV</key>
        <string>production</string>
    </dict>
</dict>
</plist>"#,
            bootstrap_binary, server_script, home_dir.display(), home_dir.display()
        );

        fs::write(&plist_file, plist_content)
            .context("Failed to write launchd plist file")?;

        info!("Plist file written to: {}", plist_file.display());

        // Load the agent
        Command::new("launchctl")
            .args(["load", &plist_file.to_string_lossy()])
            .output()
            .context("Failed to load launchd agent")?;

        info!("✅ 9Router bootstrap agent installed and loaded");
        info!("📊 Check status: launchctl list | grep 9router");
        info!("📋 View logs: tail -f ~/Library/Logs/9router-bootstrap.log");

        Ok(())
    }

    /// Uninstall launchd agent
    #[cfg(target_os = "macos")]
    pub async fn uninstall_launchd() -> Result<()> {
        info!("Uninstalling 9Router launchd agent...");

        let home_dir = dirs::home_dir().context("Could not determine home directory")?;
        let launch_agents_dir = home_dir.join("Library").join("LaunchAgents");
        let plist_file = launch_agents_dir.join("com.9router.bootstrap.plist");

        // Unload the agent
        info!("Unloading agent...");
        let _ = Command::new("launchctl")
            .args(["unload", &plist_file.to_string_lossy()])
            .output();

        // Remove plist file
        if plist_file.exists() {
            fs::remove_file(&plist_file).context("Failed to remove plist file")?;
            info!("Plist file removed: {}", plist_file.display());
        }

        info!("✅ 9Router bootstrap agent uninstalled");
        Ok(())
    }

    /// Find the 9router server script in common locations
    fn find_server_script() -> Result<String> {
        let home_dir = dirs::home_dir().context("Could not determine home directory")?;

        // Common locations for the 9router server
        let candidates = vec![
            home_dir.join(".9router").join("cli.js").to_string_lossy().to_string(),
            home_dir
                .join(".9router")
                .join("app")
                .join("custom-server.js")
                .to_string_lossy()
                .to_string(),
            "/usr/local/lib/node_modules/9router/cli.js".to_string(),
            "/usr/lib/node_modules/9router/cli.js".to_string(),
            home_dir
                .join(".npm-global")
                .join("lib")
                .join("node_modules")
                .join("9router")
                .join("cli.js")
                .to_string_lossy()
                .to_string(),
        ];

        for path in candidates {
            if PathBuf::from(&path).exists() {
                info!("Found server script: {}", path);
                return Ok(path);
            }
        }

        anyhow::bail!(
            "9Router server script not found. Please install 9router globally with: npm install -g 9router"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_server_script_does_not_panic() {
        let _ = ServiceManager::find_server_script();
    }
}

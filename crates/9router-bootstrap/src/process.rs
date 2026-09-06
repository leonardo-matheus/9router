use std::process::{Command, Stdio};
use std::time::Duration;
use tokio::process::Command as AsyncCommand;
use tracing::{info, warn};

#[derive(Clone)]
pub struct ProcessManager {
    pub node_path: String,
    node_args: String,
    pub server_path: String,
    server_args: Vec<String>,
}

impl ProcessManager {
    pub fn new(
        node_path: String,
        node_args: String,
        server_path: String,
        server_args: Vec<String>,
    ) -> Self {
        Self {
            node_path,
            node_args,
            server_path,
            server_args,
        }
    }

    pub async fn find_node() -> anyhow::Result<String> {
        if let Ok(path) = which::which("node") {
            info!("Found Node.js in PATH: {}", path.display());
            return Ok(path.to_string_lossy().to_string());
        }

        for path_pattern in [
            "/usr/local/bin/node",
            "/usr/bin/node",
            "/opt/nodejs/bin/node",
        ] {
            if let Ok(glob_matches) = glob::glob(path_pattern) {
                for entry in glob_matches.flatten() {
                    info!("Found Node.js at: {}", entry.display());
                    return Ok(entry.to_string_lossy().to_string());
                }
            }
        }

        anyhow::bail!("Node.js not found. Please install Node.js or specify --node-path")
    }

    pub async fn spawn(&self) -> anyhow::Result<u32> {
        info!("Spawning Node.js server...");

        let mut cmd = AsyncCommand::new(&self.node_path);
        for arg in self.node_args.split_whitespace() {
            cmd.arg(arg);
        }
        cmd.arg(&self.server_path);
        for arg in &self.server_args {
            cmd.arg(arg);
        }

        cmd.env("NODE_ENV", "production");
        cmd.env("NINEROUTER_BOOTSTRAP", "1");
        cmd.stdout(Stdio::inherit());
        cmd.stderr(Stdio::inherit());

        let child = cmd.spawn()?;
        let pid = child.id().unwrap_or(0);
        info!("Server spawned successfully (PID: {})", pid);
        Ok(pid)
    }

    pub async fn kill(&self) -> anyhow::Result<()> {
        warn!("Killing server process...");
        self.kill_by_pid_file().await?;
        self.kill_by_port(20128).await
    }

    async fn kill_by_pid_file(&self) -> anyhow::Result<()> {
        let Some(home) = dirs::home_dir() else {
            return Ok(());
        };
        let pid_file = home.join(".9router").join("server.pid");
        if !pid_file.exists() {
            return Ok(());
        }

        let pid_str = std::fs::read_to_string(&pid_file)?;
        let pid: u32 = pid_str.trim().parse()?;
        info!("Killing server by PID: {}", pid);

        #[cfg(unix)]
        {
            use nix::sys::signal::{kill, Signal};
            use nix::unistd::Pid;
            let _ = kill(Pid::from_raw(pid as i32), Signal::SIGTERM);
            tokio::time::sleep(Duration::from_secs(2)).await;
            let _ = kill(Pid::from_raw(pid as i32), Signal::SIGKILL);
        }

        let _ = std::fs::remove_file(pid_file);
        Ok(())
    }

    pub async fn kill_by_port(&self, port: u16) -> anyhow::Result<()> {
        info!("Killing process on port {}...", port);

        let output = Command::new("lsof")
            .args(["-sTCP:LISTEN", "-ti", &format!("tcp:{}", port)])
            .output()
            .map_err(|e| anyhow::anyhow!("Failed to execute lsof: {}", e))?;

        if output.status.success() {
            let pids = String::from_utf8_lossy(&output.stdout);
            for pid_str in pids.lines() {
                let pid_str = pid_str.trim();
                if pid_str.is_empty() {
                    continue;
                }
                if let Ok(pid) = pid_str.parse::<u32>() {
                    info!("Killing process {} on port {}", pid, port);
                    #[cfg(unix)]
                    {
                        use nix::sys::signal::{kill, Signal};
                        use nix::unistd::Pid;
                        let _ = kill(Pid::from_raw(pid as i32), Signal::SIGTERM);
                        tokio::time::sleep(Duration::from_millis(500)).await;
                        let _ = kill(Pid::from_raw(pid as i32), Signal::SIGKILL);
                    }
                }
            }
            return Ok(());
        }

        warn!("No process found on port {}", port);
        Ok(())
    }
}

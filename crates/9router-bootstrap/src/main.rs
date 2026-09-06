use clap::{Parser, Subcommand};
use std::net::SocketAddr;
use std::time::Duration;
use tracing::{error, info, warn};

mod health;
mod process;
mod retry;
mod service;

use health::HealthChecker;
use process::ProcessManager;
use retry::RetryPolicy;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Commands,

    #[arg(short, long, default_value_t = 20128)]
    port: u16,

    #[arg(short, long, default_value = "127.0.0.1")]
    host: String,

    #[arg(long, default_value_t = 3)]
    max_restarts: u32,

    #[arg(long, default_value_t = 1000)]
    retry_delay_ms: u64,

    #[arg(short, long)]
    verbose: bool,

    #[arg(long)]
    node_path: Option<String>,

    #[arg(long, default_value = "--dns-result-order=ipv4first --max-old-space-size=6144")]
    node_args: String,
}

#[derive(Subcommand, Debug)]
enum Commands {
    Start {
        server_path: String,
        #[arg(last = true)]
        server_args: Vec<String>,
    },
    HealthCheck,
    Stop,
    Status,
    InstallService,
    UninstallService,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    let log_level = if args.verbose { "debug" } else { "info" };
    tracing_subscriber::fmt()
        .with_env_filter(format!("nine_router_bootstrap={}", log_level))
        .with_target(false)
        .init();

    info!("9Router Bootstrap v{} starting", env!("CARGO_PKG_VERSION"));

    match args.command {
        Commands::Start {
            server_path,
            server_args,
        } => run_start_command(&server_path, &server_args, args.port, args.host, args.max_restarts, args.retry_delay_ms, args.verbose, args.node_path, args.node_args).await,
        Commands::HealthCheck => run_health_check(args.port, args.host).await,
        Commands::Stop => run_stop_command(args.port).await,
        Commands::Status => run_status_command(args.port, args.host).await,
        Commands::InstallService => install_service().await,
        Commands::UninstallService => uninstall_service().await,
    }
}

async fn run_start_command(
    server_path: &str,
    server_args: &[String],
    port: u16,
    host: String,
    max_restarts: u32,
    retry_delay_ms: u64,
    _verbose: bool,
    node_path: Option<String>,
    _node_args: String,
) {
    info!("Starting 9Router server: {}", server_path);

    let node_path = match node_path {
        Some(p) => p,
        None => match ProcessManager::find_node().await {
            Ok(p) => p,
            Err(e) => {
                error!("Node.js not found: {}", e);
                std::process::exit(1);
            }
        },
    };

    info!("Using Node.js: {}", node_path);

    if !std::path::Path::new(server_path).exists() {
        error!("Server script not found: {}", server_path);
        std::process::exit(1);
    }

    let manager = ProcessManager::new(
        node_path,
        _node_args,
        server_path.to_string(),
        server_args.to_vec(),
    );

    let addr: SocketAddr = format!("{}:{}", host, port)
        .parse()
        .expect("Invalid host:port combination");
    let health_checker = HealthChecker::new(addr);
    let mut retry_policy = RetryPolicy::new(max_restarts, retry_delay_ms);

    let mut attempt = 0;
    loop {
        attempt += 1;
        info!("Attempt {}/{}", attempt, max_restarts + 1);

        match manager.spawn().await {
            Ok(pid) => {
                info!("Server spawned with PID: {}", pid);

                // wait_ready() returns Ok when ready, Err on timeout (triggers retry)
                if let Err(e) = health_checker.wait_ready(Duration::from_secs(30)).await {
                    error!("Server failed to become ready: {}", e);
                    let _ = manager.kill().await;
                    continue;
                }
                info!("Server is ready and healthy");
                retry_policy.reset();

                // monitor_until_failure returns Err when server dies (triggers retry)
                let monitor_health = health_checker.clone();
                let monitor_handle = tokio::spawn(async move {
                    monitor_health.monitor_until_failure(Duration::from_secs(5)).await
                });

                if monitor_handle.await.map(|r| r.is_err()).unwrap_or(true) {
                    warn!("Server health monitor detected failure");
                    let _ = manager.kill().await;
                }
            }
            Err(e) => error!("Failed to spawn server: {}", e),
        }

        if !retry_policy.should_retry() {
            error!("Max restart attempts reached, giving up");
            std::process::exit(1);
        }

        let delay = retry_policy.next_delay();
        warn!("Restarting in {}ms...", delay.as_millis());
        tokio::time::sleep(delay).await;
    }
}

async fn run_health_check(port: u16, host: String) {
    let addr: SocketAddr = format!("{}:{}", host, port)
        .parse()
        .expect("Invalid host:port combination");
    let health_checker = HealthChecker::new(addr);

    if health_checker.check_now().await {
        info!("Server is healthy");
        std::process::exit(0);
    } else {
        error!("Server is not responding");
        std::process::exit(1);
    }
}

async fn run_stop_command(port: u16) {
    info!("Stopping 9Router server on port {}...", port);

    let manager = ProcessManager::new(
        String::new(),
        String::new(),
        String::new(),
        Vec::new(),
    );

    match manager.kill_by_port(port).await {
        Ok(_) => info!("Server stopped"),
        Err(e) => error!("Failed to stop server: {}", e),
    }
}

async fn run_status_command(port: u16, host: String) {
    let addr: SocketAddr = format!("{}:{}", host, port)
        .parse()
        .expect("Invalid host:port combination");
    let health_checker = HealthChecker::new(addr);

    if health_checker.check_now().await {
        info!("9Router is running on {}:{}", host, port);
        std::process::exit(0);
    } else {
        info!("9Router is not running on {}:{}", host, port);
        std::process::exit(1);
    }
}

async fn install_service() {
    info!("Installing 9Router systemd user service...");

    #[cfg(target_os = "linux")]
    {
        if let Err(e) = service::ServiceManager::install_systemd().await {
            error!("Failed to install systemd service: {}", e);
            std::process::exit(1);
        }
    }

    #[cfg(target_os = "macos")]
    {
        if let Err(e) = service::ServiceManager::install_launchd().await {
            error!("Failed to install launchd agent: {}", e);
            std::process::exit(1);
        }
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        error!("Service installation is only supported on Linux and macOS");
        std::process::exit(1);
    }
}

async fn uninstall_service() {
    info!("Uninstalling 9Router systemd user service...");

    #[cfg(target_os = "linux")]
    {
        if let Err(e) = service::ServiceManager::uninstall_systemd().await {
            error!("Failed to uninstall systemd service: {}", e);
            std::process::exit(1);
        }
    }

    #[cfg(target_os = "macos")]
    {
        if let Err(e) = service::ServiceManager::uninstall_launchd().await {
            error!("Failed to uninstall launchd agent: {}", e);
            std::process::exit(1);
        }
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        error!("Service uninstallation is only supported on Linux and macOS");
        std::process::exit(1);
    }
}

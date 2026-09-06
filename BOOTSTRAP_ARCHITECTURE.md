# Bootstrap Service Architecture

## Overview

The 9Router Bootstrap Service is a Rust-based system service that ensures reliable startup of the 9Router proxy server on Unix systems (Linux and macOS).

## Problem Statement

When 9Router is configured to auto-start via `.desktop` file on Linux, it frequently fails with proxy errors because:

1. **Timing**: The `.desktop` entry executes immediately at boot, before:
   - Network interfaces are fully initialized
   - Display server (X11/Wayland) is ready
   - DNS/hostname resolution is available
   - User environment variables are loaded

2. **No Retry Logic**: The current Node.js CLI has no built-in mechanism to retry startup failures.

3. **No Health Monitoring**: Once started, there's no monitoring to detect and recover from failures.

## Solution Architecture

### Rust Bootstrap Service

A lightweight Rust binary that wraps the Node.js server startup with:

- **Systemd/launchd integration**: Proper service ordering and dependencies
- **Retry logic**: Exponential backoff for startup failures
- **Health monitoring**: TCP port checking every 5 seconds
- **Auto-restart**: Automatic recovery on failure
- **Signal handling**: Graceful shutdown on SIGTERM/SIGINT/SIGHUP

### Systemd User Service (Linux)

```ini
[Unit]
Description=9Router Bootstrap Service
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
ExecStart=/path/to/nine-router-bootstrap start /path/to/cli.js
Restart=always
RestartSec=10

[Install]
WantedBy=default.target
```

Key features:
- `After=network-online.target` ensures network is ready before starting
- `Restart=always` automatically restarts on failure
- `RestartSec=10` waits 10 seconds before restarting
- Logs are captured by `journalctl --user`

### Launchd Agent (macOS)

```xml
<plist>
<key>RunAtLoad</key>
<true/>
<key>KeepAlive</key>
<true/>
</plist>
```

Key features:
- `RunAtLoad` starts at login
- `KeepAlive` restarts on failure
- Logs written to `~/Library/Logs/`

## Component Details

### 1. Health Checker (`health.rs`)

Monitors the server's TCP port to detect if it's running:

```rust
pub struct HealthChecker {
    address: String,  // "127.0.0.1:20128"
}

impl HealthChecker {
    // Check TCP connection
    pub async fn check_tcp(&self, timeout: Duration) -> bool

    // Wait until server is ready (with 30s timeout)
    pub async fn wait_ready(&self, timeout: Duration)

    // Monitor server health until failure
    pub async fn monitor_until_failure(&self, interval: Duration)
}
```

### 2. Process Manager (`process.rs`)

Manages Node.js server process lifecycle:

```rust
pub struct ProcessManager {
    node_path: String,
    node_args: String,
    server_path: String,
    server_args: Vec<String>,
}

impl ProcessManager {
    // Find Node.js in PATH or common locations
    pub async fn find_node() -> Result<String>

    // Spawn server process
    pub async fn spawn(&self) -> Result<u32>

    // Kill server by PID file
    async fn kill_by_pid_file(&self) -> Result<()>

    // Kill server by port
    pub async fn kill_by_port(&self, port: u16) -> Result<()>
}
```

### 3. Retry Policy (`retry.rs`)

Implements exponential backoff for startup retries:

```rust
pub struct RetryPolicy {
    max_attempts: u32,      // Default: 3
    current_attempt: u32,
    initial_delay: Duration, // Default: 1000ms
    max_delay: Duration,     // Default: 30s
    multiplier: f64,         // Default: 2.0
}

impl RetryPolicy {
    pub fn should_retry(&self) -> bool
    pub fn next_delay(&mut self) -> Duration  // 1s, 2s, 4s, 8s, 16s, 30s...
    pub fn reset(&mut self)  // Call after successful startup
}
```

### 4. Service Manager (`service.rs`)

Handles systemd/launchd installation:

```rust
pub struct ServiceManager;

impl ServiceManager {
    // Install systemd user service
    #[cfg(target_os = "linux")]
    pub async fn install_systemd() -> Result<()>

    // Uninstall systemd user service
    #[cfg(target_os = "linux")]
    pub async fn uninstall_systemd() -> Result<()>

    // Install launchd agent
    #[cfg(target_os = "macos")]
    pub async fn install_launchd() -> Result<()>

    // Uninstall launchd agent
    #[cfg(target_os = "macos")]
    pub async fn uninstall_launchd() -> Result<()>
}
```

## Data Flow

```
Boot
  │
  ▼
Systemd starts 9router-bootstrap.service
  │
  ├─► After network-online.target
  │
  ▼
Bootstrap spawns Node.js server
  │
  ├─► wait_ready() polls TCP port every 500ms
  │   │
  │   ├─► Success: Server is ready
  │   │   └─► monitor_until_failure() checks every 5s
  │   │
  │   └─► Timeout (30s): Kill server, retry
  │
  ▼
If server dies during monitoring:
  │
  ├─► Kill process
  ├─► Wait with exponential backoff
  └─► Retry spawn
```

## Error Handling

### Startup Failures

1. **Node.js not found**: Exit with error, no retry
2. **Server script not found**: Exit with error, no retry
3. **Port already in use**: Kill existing process, retry
4. **Server timeout (30s)**: Kill process, retry with backoff
5. **Max retries reached (3)**: Exit with error

### Runtime Failures

1. **Health check fails**: Kill process, retry with backoff
2. **SIGTERM/SIGINT received**: Graceful shutdown
3. **SIGHUP received**: Graceful shutdown

## Testing Strategy

### Unit Tests

- `test_health_checker_creation`: Verifies HealthChecker instantiation
- `test_tcp_check_timeout`: Verifies timeout behavior
- `test_retry_policy_default`: Verifies default retry settings
- `test_retry_policy_with_custom_values`: Verifies custom retry settings
- `test_retry_policy_reset`: Verifies reset functionality
- `test_retry_policy_exponential_backoff`: Verifies backoff timing
- `test_find_server_script_does_not_panic`: Verifies script finding doesn't panic
- `test_process_manager_creation`: Verifies ProcessManager instantiation

### Integration Tests

- `test_find_node_success`: Verifies Node.js detection
- `test_process_manager_creation`: Verifies manager creation with fields
- `test_retry_policy_default`: Verifies default retry behavior
- `test_retry_policy_with_custom_values`: Verifies retry sequence
- `test_retry_policy_reset`: Verifies reset after retries
- `test_retry_policy_exponential_backoff`: Verifies delay progression
- `test_spawn_nonexistent_server`: Verifies spawn error handling

## Performance Considerations

- **Memory**: ~5-10 MB RSS (Rust binary + Tokio runtime)
- **CPU**: <1% during monitoring, spikes during spawn
- **Disk**: ~2 MB binary size
- **Network**: No network usage (local TCP checks only)

## Security Considerations

- **No root privileges required**: Runs as user service
- **Process isolation**: Spawns Node.js with limited environment
- **Signal handling**: Graceful shutdown prevents zombie processes
- **No external dependencies**: Only depends on Node.js and system utilities (`lsof`)

## Future Improvements

- [ ] HTTP health check endpoint (if 9Router adds `/health`)
- [ ] Metrics export (Prometheus format)
- [ ] Configuration file support
- [ ] Windows service support
- [ ] Docker health check integration

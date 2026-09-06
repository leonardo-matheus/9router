# Fix: Linux auto-start proxy-error via Rust bootstrap service

## Problem

When 9Router is configured to auto-start on x64 Linux via a `.desktop` autostart entry, it frequently fails with proxy errors shortly after boot. Users report the application starts but cannot connect — the proxy never becomes healthy.

## Root Cause

The `.desktop` entry executes immediately during the boot sequence, before the system is fully ready:

1. **Network interfaces are not fully initialised** — DNS resolution and routing are unavailable, so the Node.js server cannot bind or reach upstream resources.
2. **Display server (X11 / Wayland) is not ready** — the Tauri frontend depends on a graphical session.
3. **User environment variables are not loaded** — `PATH`, `NODE_ENV`, and other variables that the server depends on may be missing or incomplete.
4. **No retry or recovery mechanism** — the Node.js CLI exits on failure with no automatic retry. Once the boot-time spawn fails, the user must manually restart the application.

The combination of these factors means the `.desktop` entry is fundamentally the wrong mechanism for a network-dependent service. It provides no ordering guarantees and no resilience.

## Solution: Rust Bootstrap Service

A new Rust binary (`nine-router-bootstrap`, version 0.5.69) replaces the `.desktop` auto-start path on Unix systems. It acts as a thin, reliable wrapper around the Node.js server process.

### Architecture

```
Boot
  │
  ▼
systemd starts 9router-bootstrap.service
  │
  ├─ After=network-online.target  ← network is guaranteed ready
  │
  ▼
Bootstrap spawns Node.js server
  │
  ├─ wait_ready(): TCP poll every 500 ms (30 s timeout)
  │   ├─ Success → monitor_until_failure(): TCP poll every 5 s
  │   └─ Timeout → kill process, retry
  │
  ▼
If server dies at any point:
  │
  ├─ Kill process (SIGTERM → SIGKILL with grace period)
  ├─ Exponential backoff: 1 s, 2 s, 4 s, … (capped at 30 s)
  └─ Retry up to max_restarts times (default: 3)
```

### Key components

| Module | File | Responsibility |
|--------|------|----------------|
| HealthChecker | `src/health.rs` | TCP-only health probes — no HTTP dependency. Polls `127.0.0.1:<port>` to detect readiness and liveness. |
| ProcessManager | `src/process.rs` | Spawns Node.js with `NODE_ENV=production`. Kills via PID-file (`~/.9router/server.pid`) or `lsof` port lookup. Uses `nix` signals (`SIGTERM` → `SIGKILL`). |
| RetryPolicy | `src/retry.rs` | Exponential backoff with configurable max attempts and initial delay. Capped at 30 s. |
| ServiceManager | `src/service.rs` | Installs / uninstalls the systemd user service (Linux) or launchd agent (macOS). Discovers the server script from common install paths. |

### systemd user service (Linux)

The `install-service` command writes `~/.config/systemd/user/9router-bootstrap.service` with:

```ini
After=network-online.target
Wants=network-online.target
Restart=always
RestartSec=10
```

This ensures the bootstrap does not start until networking is confirmed, and systemd restarts it automatically on any crash. Logs are captured by `journalctl --user -u 9router-bootstrap.service`.

### launchd agent (macOS)

The `install-service` command writes `~/Library/LaunchAgents/com.9router.bootstrap.plist` with:

```xml
<key>RunAtLoad</key>   <true/>
<key>KeepAlive</key>    <true/>
```

### New CLI surface

```
nine-router-bootstrap start <script> [args...]   # main entry point
  --port, --host, --max-restarts, --retry-delay-ms, --node-path, --verbose

nine-router-bootstrap health-check   # one-shot TCP probe
nine-router-bootstrap stop            # kill by port
nine-router-bootstrap status          # report running / not running
nine-router-bootstrap install-service # systemd or launchd
nine-router-bootstrap uninstall-service
```

## Test Results

### Unit tests (pass)

| Test | Module | Result |
|------|--------|--------|
| `test_health_checker_creation` | `health.rs` | pass |
| `test_tcp_check_timeout` | `health.rs` | pass |
| `test_retry_policy_default` | `retry.rs` | pass |
| `test_retry_policy_with_custom_values` | `retry.rs` | pass |
| `test_retry_policy_reset` | `retry.rs` | pass |
| `test_retry_policy_exponential_backoff` | `retry.rs` | pass |
| `test_find_server_script_does_not_panic` | `service.rs` | pass |
| `test_process_manager_creation` | `process.rs` | pass (via integration tests) |

### Integration tests (pass)

| Test | What it covers |
|------|----------------|
| `test_find_node_success` | Node.js detection via `which` and glob fallback |
| `test_process_manager_creation` | Manager instantiation with field verification |
| `test_retry_policy_default` | Default 3 attempts, correct initial state |
| `test_retry_policy_with_custom_values` | Custom 5 attempts with 500 ms base delay |
| `test_retry_policy_reset` | Counter resets to 0 after successful startup |
| `test_retry_policy_exponential_backoff` | Delay 2 > delay 1, delay 3 > delay 2 |
| `test_spawn_nonexistent_server` | Confirms `spawn()` does not validate script existence (that is done separately) |

### Manual verification

```bash
# Build
cargo build --package nine-router-bootstrap --release

# Install systemd service
./target/release/nine-router-bootstrap install-service

# Check status
systemctl --user status 9router-bootstrap.service

# Tail logs
journalctl --user -u 9router-bootstrap.service -f
```

## Unix Compatibility Notes

| Platform | Support level | Service manager | Notes |
|----------|--------------|-----------------|-------|
| **x64 Linux** (systemd) | Full | `systemd --user` | Requires `lsof` on PATH. `network-online.target` ensures boot-order correctness. |
| **x64 macOS** | Full | `launchd` | `RunAtLoad` + `KeepAlive` provide equivalent guarantees. Logs to `~/Library/Logs/`. |
| **Windows** | Not supported | N/A | All Unix-gated code (`cfg(unix)`); does not compile on Windows. |

The crate targets Unix only. All signal handling (`nix::sys::signal`) and `lsof`-based port cleanup are behind `#[cfg(unix)]` guards.

## Files Changed

- **New:** `crates/9router-bootstrap/` — Rust crate with `src/{health,process,retry,service,main}.rs`, `Cargo.toml`, `README.md`, `tests/integration_test.rs`.
- **New:** `crates/9router-bootstrap/CONTRIBUTING.md` — build prerequisites, testing, PR guidelines.
- **New:** `RELEASE-NOTES.md` — user-facing release notes.
- **Unchanged:** `9router-tauri/` — no modifications to the Tauri frontend.

## Migration

Existing users with a `.desktop` autostart entry should remove it and run:

```bash
nine-router-bootstrap install-service
```

This handles the full transition — installing the systemd user service, enabling it, and starting it immediately.

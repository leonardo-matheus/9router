# Release Notes — 9Router Bootstrap Fix

## Version 0.5.69

### Linux auto-start proxy-error fix

On x64 Linux, 9Router configured to launch via a `.desktop` autostart entry frequently failed with proxy errors shortly after boot. The root cause was a **timing gap**: the `.desktop` entry fires before the network stack, display server, and user environment are fully initialised, and the Node.js server has no built-in retry or health-monitoring logic to recover.

### New: Rust bootstrap service (`nine-router-bootstrap`)

A new Rust binary replaces the fragile auto-start path with a proper systemd user service (Linux) and a launchd agent (macOS).

**What changed:**

- **systemd `After=network-online.target`** — the bootstrap service now waits for the network before attempting to start the Node.js server, eliminating the boot-time race.
- **Exponential-backoff retry logic** — if the server fails to start (or crashes after starting), the bootstrap retries up to 3 times with delays of 1 s, 2 s, 4 s, … capped at 30 s.
- **TCP health monitoring** — a 500 ms polling loop on `127.0.0.1:20128` confirms the server is ready before marking startup successful, then switches to a 5-second monitoring interval. If the server stops responding, the bootstrap kills the process and enters the retry cycle.
- **Graceful signal handling** — `SIGTERM`, `SIGINT`, and `SIGHUP` trigger clean shutdown with `SIGTERM` → `SIGKILL` escalation after a grace period.
- **Process cleanup** — orphaned processes are killed via PID-file lookup or `lsof` port lookup, preventing zombie and port-conflict scenarios.

**New CLI subcommands:**

| Subcommand | Purpose |
|------------|---------|
| `start <script> [args...]` | Start the Node.js server with retry and monitoring |
| `health-check` | One-shot TCP health probe (exit 0 / 1) |
| `stop` | Kill the server by port |
| `status` | Report running / not running |
| `install-service` | Install systemd user service or launchd agent |
| `uninstall-service` | Remove the installed service |

**Configuration flags:** `--port`, `--host`, `--max-restarts`, `--retry-delay-ms`, `--node-path`, `--verbose`.

### Unix compatibility

- **x64 Linux (systemd):** fully supported. Install via `nine-router-bootstrap install-service`; uninstall via `uninstall-service`. Logs: `journalctl --user -u 9router-bootstrap.service -f`.
- **x64 macOS (launchd):** fully supported. Install creates `~/Library/LaunchAgents/com.9router.bootstrap.plist` with `RunAtLoad` and `KeepAlive`. Logs: `~/Library/Logs/9router-bootstrap.log`.
- **Windows:** not supported. The crate compiles only on Unix targets (`cfg(unix)` gating throughout).

### Memory / performance

Approximately 5–10 MB RSS, <1 % CPU during monitoring, ~2 MB binary size. No external network usage — health checks are local TCP only.

### Upgrade path

Existing `.desktop` autostart entries should be replaced with the systemd user service (Linux) or launchd agent (macOS). Run `install-service` from the new bootstrap binary to set this up automatically.

---

# Contributing

See `crates/9router-bootstrap/CONTRIBUTING.md` for build prerequisites, testing instructions, and PR guidelines.

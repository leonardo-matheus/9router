# 9Router Bootstrap Service

Rust-based bootstrap service for 9Router that ensures reliable startup on Unix systems (Linux, macOS).

## Problem

When 9Router is configured to auto-start on Linux via `.desktop` file, it often fails with proxy errors because:
- Network is not fully initialized
- Display server (X11/Wayland) is not ready
- Environment variables are not loaded
- DNS/hostname resolution is not available

This service solves that by using **systemd user services** with proper dependency ordering and retry logic.

## Features

- **Systemd user service** (Linux) / **launchd agent** (macOS)
- **Retry logic** with exponential backoff
- **Health checks** (TCP port monitoring)
- **Auto-restart** on failure
- **Signal handling** (SIGTERM, SIGINT, SIGHUP)
- **Process cleanup** (kills orphaned processes)
- **Structured logging** via `tracing`

## Installation

### Prerequisites

- Rust 1.75 or later
- Node.js (for running 9Router server)
- 9Router installed globally: `npm install -g 9router`

### Build from Source

```bash
# Clone the repository
git clone https://github.com/decolua/9router.git
cd 9router/crates/9router-bootstrap

# Build release binary
cargo build --release

# Binary will be at: target/release/nine-router-bootstrap
```

### Install as Systemd Service

```bash
# Install the service
./target/release/nine-router-bootstrap install-service

# Check status
systemctl --user status 9router-bootstrap.service

# View logs
journalctl --user -u 9router-bootstrap.service -f
```

### Uninstall

```bash
# Uninstall the service
./target/release/nine-router-bootstrap uninstall-service
```

## Usage

### Start 9Router with Bootstrap

```bash
# Start with default settings
nine-router-bootstrap start /usr/local/lib/node_modules/9router/cli.js --tray --skip-update

# Custom port and host
nine-router-bootstrap start --port 20128 --host 127.0.0.1 /path/to/server.js

# Custom retry settings
nine-router-bootstrap start --max-restarts 5 --retry-delay-ms 500 /path/to/server.js
```

### Health Check

```bash
# Check if 9Router is running
nine-router-bootstrap health-check

# Check status
nine-router-bootstrap status
```

### Stop Service

```bash
# Stop 9Router
nine-router-bootstrap stop
```

## How It Works

### Linux (systemd)

1. Creates a systemd user service at `~/.config/systemd/user/9router-bootstrap.service`
2. Service has `After=network-online.target` dependency
3. Runs with `Restart=always` and `RestartSec=10`
4. Logs are available via `journalctl --user -u 9router-bootstrap.service`

### macOS (launchd)

1. Creates a launchd agent at `~/Library/LaunchAgents/com.9router.bootstrap.plist`
2. Agent has `RunAtLoad=true` and `KeepAlive=true`
3. Logs are written to `~/Library/Logs/9router-bootstrap.log`

### Process Management

1. **Spawn**: Starts Node.js server process
2. **Health Check**: Monitors TCP port every 500ms
3. **Wait Ready**: Blocks until server responds or timeout (30s)
4. **Monitor**: Continuously checks server health every 5s
5. **Retry**: On failure, waits with exponential backoff (1s, 2s, 4s, ... up to 30s)
6. **Cleanup**: Kills server process on shutdown

## Architecture

```
┌─────────────────────────────────────────┐
│   Systemd User Service / launchd        │
│   (starts on boot, after network)       │
└──────────────┬──────────────────────────┘
               │
               ▼
┌─────────────────────────────────────────┐
│   9Router Bootstrap (Rust)              │
│   ┌─────────────────────────────────┐  │
│   │  Retry Policy (exponential)     │  │
│   └─────────────────────────────────┘  │
│   ┌─────────────────────────────────┐  │
│   │  Health Checker (TCP monitor)   │  │
│   └─────────────────────────────────┘  │
│   ┌─────────────────────────────────┐  │
│   │  Process Manager (spawn/kill)   │  │
│   └─────────────────────────────────┘  │
└──────────────┬──────────────────────────┘
               │
               ▼
┌─────────────────────────────────────────┐
│   Node.js Server (9Router)              │
│   http://127.0.0.1:20128                │
└─────────────────────────────────────────┘
```

## Testing

```bash
# Run all tests
cargo test

# Run with output
cargo test -- --nocapture

# Run specific test
cargo test test_health_checker_creation
```

## Contributing

This is part of the 9Router project. See [CONTRIBUTING.md](https://github.com/decolua/9router/blob/main/CONTRIBUTING.md) for guidelines.

## License

MIT - see [LICENSE](../LICENSE)

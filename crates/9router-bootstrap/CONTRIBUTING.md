# Contributing to `9router-bootstrap`

## Build Prerequisites

| Dependency | Minimum Version | Notes |
|------------|-----------------|-------|
| Rust       | 1.75            | Edition 2021; `rustup default 1.75` or newer |
| Node.js    | 16+             | Required at runtime — the bootstrap wraps a Node.js server |
| systemd    | any             | Linux only — required for service install/uninstall |
| lsof       | any             | Linux only — used for port-based process cleanup |

Install Rust via [rustup](https://rustup.rs/). The bootstrap crate lives at `crates/9router-bootstrap/`.

## Building

```bash
# From the workspace root
cargo build --workspace

# Release binary only for the bootstrap crate
cargo build --package nine-router-bootstrap --release

# Binary output
#   target/release/nine-router-bootstrap
```

## Testing

```bash
# Unit + integration tests across the workspace
cargo test --workspace

# Bootstrap crate only
cargo test --package nine-router-bootstrap

# With full output (useful for tracing logs)
cargo test --package nine-router-bootstrap -- --nocapture

# Run a single test
cargo test --package nine-router-bootstrap test_health_checker_creation
```

### Test Categories

| Category | Location | What it covers |
|----------|----------|----------------|
| Unit | `src/health.rs`, `src/retry.rs`, `src/service.rs` | HealthChecker, RetryPolicy, script discovery |
| Integration | `tests/integration_test.rs` | Node.js detection, ProcessManager, retry sequences, spawn error handling |
| Platform | `#[cfg(unix)]` blocks in `process.rs` | Signal-based process cleanup (Linux/macOS) |

### Known Test Assumptions

- `test_find_node_success` requires `node` to be on `PATH` (or at a glob-matched path). It will fail in CI environments without Node.js.
- `test_spawn_nonexistent_server` asserts that `spawn()` returns `Ok` because script existence is validated separately before `spawn()` is called — this is intentional.

## PR Guidelines

1. **Keep the Unix-only boundary clean.** All Windows-specific code should be gated behind `#[cfg(unix)]` or `#[cfg(not(unix))]`. Do not add `std::os::unix` code outside those guards.
2. **Do not change the CLI surface without updating `BOOTSTRAP_ARCHITECTURE.md`.** The `start`, `health-check`, `stop`, `status`, `install-service`, and `uninstall-service` subcommands, plus `--port`, `--host`, `--max-restarts`, and `--retry-delay-ms` flags, are part of the documented interface.
3. **Preserve the TCP-only health check contract.** The health checker uses raw `TcpStream` connections — do not introduce HTTP or dependency-heavy checks without discussion.
4. **Log with `tracing`**, not `println!`. Use `info!`, `warn!`, `error!`, `debug!` appropriately.
5. **Process cleanup must be safe.** The `kill_by_pid_file` and `kill_by_port` methods use `SIGTERM` followed by `SIGKILL` with a grace period. Any change to signal timing or order should be reviewed for zombie-process risk.
6. **Add tests for every new public function or significant logic change.** Place unit tests in the same module (`#[cfg(test)]`); place integration tests in `tests/integration_test.rs`.
7. **Do not modify `9router-tauri/`.** The Tauri frontend is a separate project and out of scope for bootstrap changes.
8. **Update `RELEASE-NOTES.md`** and **`PR.md`** when submitting a PR that fixes a user-facing bug or adds a feature.

# CancerBroker

[Language Index](readme-pages/index.md) | [English](readme-pages/english.md) | [中文](readme-pages/chinese.md) | [Español](readme-pages/spanish.md) | [한국어](readme-pages/korean.md) | [日本語](readme-pages/japanese.md)

RAM optimizer for Opencode subagents and background helper processes.

## Quick Start

```bash
cargo run -- --config fixtures/config/observe-only.toml status --json
```

## Architecture

- **Process tracking**: Collects live process snapshots via `sysinfo`, tracking PID, parent PID, process group ID, listening ports, UID, memory, and CPU.
- **Session resolution**: Maps Opencode session IDs to their process trees and storage artifacts using command-line pattern matching.
- **Safety**: Ownership validation ensures only processes matching UID and command markers are candidates for cleanup.
- **Remediation**: Graceful SIGTERM with configurable timeout, escalating to SIGKILL. Supports both per-process and process-group signaling.
- **File cleanup**: Allowlist-based deletion of stale session artifacts with pre-action evidence recording.
- **Policy engine**: Quorum and budget controls govern automated cleanup decisions.
- **Daemon mode**: Long-running daemon with Unix socket IPC for real-time status queries.
- **Packaging**: Generates systemd and launchd service templates for production deployment.

## Verification

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

# CancerBroker

[Language Index](readme-pages/index.md) | [English](readme-pages/english.md) | [中文](readme-pages/chinese.md) | [Español](readme-pages/spanish.md) | [한국어](readme-pages/korean.md) | [日本語](readme-pages/japanese.md)

CancerBroker is a Rust cleanup tool for Opencode processes. It tracks PID, PGID, listening ports, and detailed open resources, detects repeated RSS growth, and cleans up task-scoped processes with safety checks before sending signals.

## Installation

```bash
cargo install --git https://github.com/Topabaem05/CancerBroker.git
```

## Opencode Setup

```bash
cancerbroker setup
```

This registers CancerBroker as a local Opencode MCP server using `cancerbroker mcp`.

## Quick Start

```bash
cancerbroker --config fixtures/config/observe-only.toml status --json
cancerbroker --config fixtures/config/observe-only.toml run-once --json
cancerbroker --config fixtures/config/completion-cleanup.toml daemon --json --max-events 128
```

## What It Does

- Tracks live process identity with PID, parent PID, PGID, UID, memory, CPU, and listening ports.
- Resolves Opencode-related processes and session artifacts with command-marker safety rules.
- Captures detailed open files and socket endpoints before cleanup.
- Detects live RSS leak candidates and enforces cleanup in daemon mode.
- Terminates targets with `SIGTERM` first, then escalates to `SIGKILL` if they ignore the timeout.

## Verification

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
cargo build --workspace
```

## Sandbox Termination Proof

Focused test for the leak-enforcement PID kill path:

```bash
cargo test --workspace run_leak_enforcement_with_inventory_terminates_leaking_process_in_enforce_mode -- --nocapture
```

Expected signal outcomes from sandbox verification:

```json
{"returncode": -15, "signal": 15}
{"returncode": -9, "signal": 9}
```

- `signal: 15` means the target exited after `SIGTERM`.
- `signal: 9` means the target ignored `SIGTERM` and CancerBroker escalated to `SIGKILL`.

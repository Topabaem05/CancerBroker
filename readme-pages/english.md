# English

- [Back to Home](../README.md)
- [Language Index](index.md)

Languages: [English](english.md) | [中文](chinese.md) | [Español](spanish.md) | [한국어](korean.md) | [日本語](japanese.md)

## Installation

- Install from Git:

```bash
cargo install --git https://github.com/Topabaem05/CancerBroker.git
```

## Opencode Setup

```bash
cancerbroker setup
```

## Usage

- Check the current mode:

```bash
cancerbroker --config fixtures/config/observe-only.toml status --json
```

- Run one policy evaluation and write evidence under `.sisyphus/evidence`:

```bash
cancerbroker --config fixtures/config/observe-only.toml run-once --json
```

- Start the long-running completion cleanup daemon:

```bash
cancerbroker --config fixtures/config/completion-cleanup.toml daemon --json --max-events 128
```

## Sandbox PID Termination Proof

```bash
cargo test --workspace run_leak_enforcement_with_inventory_terminates_leaking_process_in_enforce_mode -- --nocapture
```

Signal interpretation:

- `signal: 15` -> terminated by `SIGTERM`
- `signal: 9` -> escalated to `SIGKILL`

## Verification

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
cargo build --workspace
```

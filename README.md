# opencode-guardian

Rust sidecar watchdog for `opencode` and `oh-my-openagent`.

## Scope

- v1 platform support: macOS + Linux
- default mode: observe-only
- conservative cleanup only for allowlisted session artifacts

## Non-goals (v1)

- Windows support
- remote control APIs
- broad project-directory cleanup

## Quick Start

```bash
cargo run -- --config fixtures/config/observe-only.toml status --json
```

## Verification

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

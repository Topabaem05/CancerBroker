# CancerBroker

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

## README by Language

- [Language Index](readme-pages/index.md)
- [English](readme-pages/english.md)
- [中文](readme-pages/chinese.md)
- [Español](readme-pages/spanish.md)
- [한국어](readme-pages/korean.md)
- [日本語](readme-pages/japanese.md)

The Korean docs also include a separate plugin guide page.

## Verification

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

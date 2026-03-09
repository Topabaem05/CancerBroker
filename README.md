# CancerBroker

[Language Index](readme-pages/index.md) | [English](readme-pages/english.md) | [中文](readme-pages/chinese.md) | [Español](readme-pages/spanish.md) | [한국어](readme-pages/korean.md) | [日本語](readme-pages/japanese.md)

Rust sidecar watchdog for `opencode` and `oh-my-openagent`.

## Tool Install

The installer adds a global custom tool named `session_memory` under `~/.config/opencode/tools/` by default.

Curl:

```bash
curl -fsSL https://raw.githubusercontent.com/Topabaem05/CancerBroker/main/install/opencode-session-memory-sidebar.sh | sh
opencode --restart
```

Homebrew:

```bash
brew install topabaem05/cancerbroker/opencode-session-memory-sidebar-installer
opencode --restart
```

If Homebrew needs an explicit tap URL:

```bash
brew tap topabaem05/cancerbroker https://github.com/Topabaem05/CancerBroker
brew install topabaem05/cancerbroker/opencode-session-memory-sidebar-installer
opencode --restart
```

## Architecture

- Distribution: GitHub Releases publish two assets, the installer `CancerBroker.cjs` and the tool file `session_memory.js`.
- Installation: the installer writes `session_memory.js` into `~/.config/opencode/tools/` by default, or `.opencode/tools/` when `--project` is used.
- Runtime loading: OpenCode loads that local file as a custom tool at startup, so no npm publication is required for the default path.
- Session data: the tool calls OpenCode session APIs through `@opencode-ai/sdk` with the current directory as scope, so live/stored session results are project-scoped rather than machine-wide.
- Memory data: RAM attribution is exact only for OpenCode session processes with a usable PID and matching start time; unrelated processes are not counted as session RAM.
- Helper view: the tool also inspects macOS process snapshots to summarize Opencode-owned helper processes such as `biome`, `typescript-language-server`, `tsserver`, and `context7-mcp`.
- Cleanup: stale duplicate Opencode-owned helper processes are cleaned conservatively using parent-chain/process-group ownership checks before signaling them.
- Current limitation: OpenCode 1.2.22 does not expose a public plugin sidebar API, so Session Memory is delivered as the `session_memory` tool rather than a sidebar panel.

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

# English

- [Back to Home](../README.md)
- [Language Index](index.md)

Languages: [English](english.md) | [中文](chinese.md) | [Español](spanish.md) | [한국어](korean.md) | [日本語](japanese.md)

## Installation

- Clone the repository and build the binary:

```bash
git clone https://github.com/Topabaem05/CancerBroker.git
cd CancerBroker
cargo build --release
```

- Install the OpenCode Session Memory plugin without cloning:

```bash
curl -fsSL https://raw.githubusercontent.com/Topabaem05/CancerBroker/main/install/opencode-session-memory-sidebar.sh | sh
opencode --restart
```

- Install it with Homebrew:

```bash
brew install topabaem05/cancerbroker/opencode-session-memory-sidebar-installer
opencode --restart
```

- OpenCode 1.2.22 currently loads this as a supported custom tool (`session_memory`), not a custom sidebar panel.

- If Homebrew needs an explicit tap URL:

```bash
brew tap topabaem05/cancerbroker https://github.com/Topabaem05/CancerBroker
brew install topabaem05/cancerbroker/opencode-session-memory-sidebar-installer
opencode --restart
```

## Usage

- Check the current mode:

```bash
cargo run -- --config fixtures/config/observe-only.toml status --json
```

- Run one policy evaluation and write evidence under `.sisyphus/evidence`:

```bash
cargo run -- --config fixtures/config/observe-only.toml run-once --json
```

- Start the long-running completion cleanup daemon:

```bash
cargo run -- --config fixtures/config/completion-cleanup.toml daemon --json --max-events 128
```

## Verification

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

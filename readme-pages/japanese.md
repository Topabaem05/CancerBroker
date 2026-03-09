# 日本語

- [Back to Home](../README.md)
- [Language Index](index.md)

Languages: [English](english.md) | [中文](chinese.md) | [Español](spanish.md) | [한국어](korean.md) | [日本語](japanese.md)

## Installation

- リポジトリをクローンしてバイナリをビルドします:

```bash
git clone https://github.com/Topabaem05/CancerBroker.git
cd CancerBroker
cargo build --release
```

- リポジトリをクローンせずに OpenCode Session Memory ツールをインストールします:

```bash
curl -fsSL https://raw.githubusercontent.com/Topabaem05/CancerBroker/main/install/opencode-session-memory-sidebar.sh | sh
opencode --restart
```

- Homebrew でインストールします:

```bash
brew install topabaem05/cancerbroker/opencode-session-memory-sidebar-installer
opencode --restart
```

- OpenCode 1.2.22 では、これは `~/.config/opencode/tools/` に入るグローバルカスタムツール `session_memory` として読み込まれます。

- Homebrew で明示的な tap URL が必要な場合:

```bash
brew tap topabaem05/cancerbroker https://github.com/Topabaem05/CancerBroker
brew install topabaem05/cancerbroker/opencode-session-memory-sidebar-installer
opencode --restart
```

## Usage

- 現在の動作モードを確認します:

```bash
cargo run -- --config fixtures/config/observe-only.toml status --json
```

- ポリシー評価を1回実行し、`.sisyphus/evidence` に証跡を書き出します:

```bash
cargo run -- --config fixtures/config/observe-only.toml run-once --json
```

- 長時間稼働する completion cleanup daemon を起動します:

```bash
cargo run -- --config fixtures/config/completion-cleanup.toml daemon --json --max-events 128
```

## Verification

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

# 中文

- [Back to Home](../README.md)
- [Language Index](index.md)

Languages: [English](english.md) | [中文](chinese.md) | [Español](spanish.md) | [한국어](korean.md) | [日本語](japanese.md)

## Installation

- 克隆仓库并构建二进制文件：

```bash
git clone https://github.com/Topabaem05/CancerBroker.git
cd CancerBroker
cargo build --release
```

- 无需克隆即可安装 OpenCode Session Memory 插件：

```bash
curl -fsSL https://raw.githubusercontent.com/Topabaem05/CancerBroker/main/install/opencode-session-memory-sidebar.sh | sh
opencode --restart
```

- 通过 Homebrew 安装：

```bash
brew install topabaem05/cancerbroker/opencode-session-memory-sidebar-installer
opencode --restart
```

- 在 OpenCode 1.2.22 中，它会作为受支持的自定义工具 `session_memory` 加载，而不是自定义侧边栏面板。

- 如果 Homebrew 需要显式 tap URL：

```bash
brew tap topabaem05/cancerbroker https://github.com/Topabaem05/CancerBroker
brew install topabaem05/cancerbroker/opencode-session-memory-sidebar-installer
opencode --restart
```

## Usage

- 检查当前运行模式：

```bash
cargo run -- --config fixtures/config/observe-only.toml status --json
```

- 执行一次策略检查，并将证据写入 `.sisyphus/evidence`：

```bash
cargo run -- --config fixtures/config/observe-only.toml run-once --json
```

- 启动长期运行的 completion cleanup daemon：

```bash
cargo run -- --config fixtures/config/completion-cleanup.toml daemon --json --max-events 128
```

## Verification

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

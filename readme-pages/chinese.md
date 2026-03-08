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

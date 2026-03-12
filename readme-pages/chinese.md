# 中文

- [Back to Home](../README.md)
- [Language Index](index.md)

Languages: [English](english.md) | [中文](chinese.md) | [Español](spanish.md) | [한국어](korean.md) | [日本語](japanese.md)

## Installation

- 通过 Git 安装：

```bash
cargo install --git https://github.com/Topabaem05/CancerBroker.git
```

## Opencode 设置

```bash
cancerbroker setup
```

## Usage

- 检查当前运行模式：

```bash
cancerbroker --config fixtures/config/observe-only.toml status --json
```

- 执行一次策略检查，并将证据写入 `~/.local/share/cancerbroker/evidence`：

```bash
cancerbroker --config fixtures/config/observe-only.toml run-once --json
```

- 启动长期运行的 completion cleanup daemon：

```bash
cancerbroker --config fixtures/config/completion-cleanup.toml daemon --json --max-events 128
```

## 沙盒 PID 终止验证

```bash
cargo test --workspace run_leak_enforcement_with_inventory_terminates_leaking_process_in_enforce_mode -- --nocapture
```

信号说明：

- `signal: 15` -> 通过 `SIGTERM` 结束
- `signal: 9` -> 在 `SIGTERM` 后升级为 `SIGKILL`

## Verification

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
cargo build --workspace
```

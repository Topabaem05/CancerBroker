# 日本語

- [Back to Home](../README.md)
- [Language Index](index.md)

Languages: [English](english.md) | [中文](chinese.md) | [Español](spanish.md) | [한국어](korean.md) | [日本語](japanese.md)

## Installation

- Git からインストールします:

```bash
cargo install --git https://github.com/Topabaem05/CancerBroker.git
```

## Opencode 設定

```bash
cancerbroker setup
```

## Usage

- 現在の動作モードを確認します:

```bash
cancerbroker --config fixtures/config/observe-only.toml status --json
```

- ポリシー評価を1回実行し、`~/.local/share/cancerbroker/evidence` に証跡を書き出します:

```bash
cancerbroker --config fixtures/config/observe-only.toml run-once --json
```

- 長時間稼働する completion cleanup daemon を起動します:

```bash
cancerbroker --config fixtures/config/completion-cleanup.toml daemon --json --max-events 128
```

## サンドボックス PID 終了検証

```bash
cargo test --workspace run_leak_enforcement_with_inventory_terminates_leaking_process_in_enforce_mode -- --nocapture
```

シグナルの意味:

- `signal: 15` -> `SIGTERM` で終了
- `signal: 9` -> `SIGTERM` 後に `SIGKILL` へ昇格

## Verification

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
cargo build --workspace
```

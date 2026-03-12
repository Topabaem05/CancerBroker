# 한국어

- [Back to Home](../README.md)
- [Language Index](index.md)

Languages: [English](english.md) | [中文](chinese.md) | [Español](spanish.md) | [한국어](korean.md) | [日本語](japanese.md)

## Installation

- Git 저장소에서 설치합니다:

```bash
cargo install --git https://github.com/Topabaem05/CancerBroker.git
```

## Opencode 설정

```bash
cancerbroker setup
```

## Usage

- 현재 동작 모드를 확인합니다:

```bash
cancerbroker --config fixtures/config/observe-only.toml status --json
```

- 정책 평가를 한 번 실행하고 `~/.local/share/cancerbroker/evidence` 아래에 증거를 기록합니다:

```bash
cancerbroker --config fixtures/config/observe-only.toml run-once --json
```

- 장기 실행 completion cleanup daemon을 시작합니다:

```bash
cancerbroker --config fixtures/config/completion-cleanup.toml daemon --json --max-events 128
```

## 샌드박스 PID 종료 검증

```bash
cargo test --workspace run_leak_enforcement_with_inventory_terminates_leaking_process_in_enforce_mode -- --nocapture
```

signal 해석:

- `signal: 15` -> `SIGTERM`으로 정상 종료
- `signal: 9` -> `SIGTERM` 이후 `SIGKILL`로 강제 종료

## Verification

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
cargo build --workspace
```

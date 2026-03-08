# 한국어

- [Back to Home](../README.md)
- [Language Index](index.md)

Languages: [English](english.md) | [中文](chinese.md) | [Español](spanish.md) | [한국어](korean.md) | [日本語](japanese.md)

## Installation

- 저장소를 클론한 뒤 바이너리를 빌드합니다:

```bash
git clone https://github.com/Topabaem05/CancerBroker.git
cd CancerBroker
cargo build --release
```

## Usage

- 현재 동작 모드를 확인합니다:

```bash
cargo run -- --config fixtures/config/observe-only.toml status --json
```

- 정책 평가를 한 번 실행하고 `.sisyphus/evidence` 아래에 증거를 기록합니다:

```bash
cargo run -- --config fixtures/config/observe-only.toml run-once --json
```

- 장기 실행 completion cleanup daemon을 시작합니다:

```bash
cargo run -- --config fixtures/config/completion-cleanup.toml daemon --json --max-events 128
```

## OpenCode Session Memory Sidebar Plugin

- 한국어 플러그인 가이드는 별도 페이지로 분리했습니다.
- [한국어 플러그인 가이드](korean-plugin-guide.md)

## Verification

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

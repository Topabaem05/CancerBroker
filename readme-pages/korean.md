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

- OpenCode Session Memory 도구를 clone 없이 설치합니다:

```bash
curl -fsSL https://raw.githubusercontent.com/Topabaem05/CancerBroker/main/install/opencode-session-memory-sidebar.sh | sh
opencode --restart
```

- Homebrew로 설치합니다:

```bash
brew install topabaem05/cancerbroker/opencode-session-memory-sidebar-installer
opencode --restart
```

- OpenCode 1.2.22에서는 기본적으로 `~/.config/opencode/tools/`에 설치되는 전역 custom tool `session_memory`로 로드됩니다.

- Homebrew에 explicit tap URL이 필요하면:

```bash
brew tap topabaem05/cancerbroker https://github.com/Topabaem05/CancerBroker
brew install topabaem05/cancerbroker/opencode-session-memory-sidebar-installer
opencode --restart
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

## OpenCode Session Memory Tool

- 한국어 플러그인 가이드는 별도 페이지로 분리했습니다.
- [한국어 플러그인 가이드](korean-plugin-guide.md)

## Verification

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

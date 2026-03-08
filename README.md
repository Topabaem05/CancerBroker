# opencode-guardian

Rust sidecar watchdog for `opencode` and `oh-my-openagent`.

## Scope

- v1 platform support: macOS + Linux
- default mode: observe-only
- conservative cleanup only for allowlisted session artifacts

## Non-goals (v1)

- Windows support
- remote control APIs
- broad project-directory cleanup

## Quick Start

```bash
cargo run -- --config fixtures/config/observe-only.toml status --json
```

## Installation

### English

- Clone the repository and build the binary:

```bash
git clone https://github.com/Topabaem05/CancerBroker.git
cd CancerBroker
cargo build --release
```

### Chinese

- 克隆仓库并构建二进制文件：

```bash
git clone https://github.com/Topabaem05/CancerBroker.git
cd CancerBroker
cargo build --release
```

### Spanish

- Clona el repositorio y compila el binario:

```bash
git clone https://github.com/Topabaem05/CancerBroker.git
cd CancerBroker
cargo build --release
```

### Korean

- 저장소를 클론한 뒤 바이너리를 빌드합니다:

```bash
git clone https://github.com/Topabaem05/CancerBroker.git
cd CancerBroker
cargo build --release
```

### Japanese

- リポジトリをクローンしてバイナリをビルドします:

```bash
git clone https://github.com/Topabaem05/CancerBroker.git
cd CancerBroker
cargo build --release
```

## Usage

### English

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

### Chinese

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

### Spanish

- Verifica el modo actual:

```bash
cargo run -- --config fixtures/config/observe-only.toml status --json
```

- Ejecuta una evaluacion de politica y escribe evidencia en `.sisyphus/evidence`:

```bash
cargo run -- --config fixtures/config/observe-only.toml run-once --json
```

- Inicia el daemon de limpieza de completion en ejecucion continua:

```bash
cargo run -- --config fixtures/config/completion-cleanup.toml daemon --json --max-events 128
```

### Korean

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

## OpenCode Session Memory Sidebar Plugin (Korean)

- 사이드바 플러그인 목적:
  - 현재 세션 + 현재 열려있는 live 세션들의 메모리 상태를 `Session Memory` 패널로 표시합니다.
  - 토큰/컨텍스트 사용량과 RAM 상태를 함께 보여주며, 공유 프로세스 등 정확한 귀속이 불가능한 경우 숫자 대신 `unavailable` 상태를 표시합니다.

- 설치 위치 (전역):

```bash
ls ~/.config/opencode/plugins/opencode-session-memory-sidebar.ts
ls ~/.config/opencode/plugins/opencode-session-memory-sidebar
```

- 플러그인 테스트:

```bash
cd ~/.config/opencode/plugins/opencode-session-memory-sidebar
bun install
bun test
```

- OpenCode 재시작:

```bash
opencode --restart
```

- 화면에서 확인할 항목:
  - 패널 제목: `Session Memory`
  - 요약 항목: `Live`, `Exact RAM`, `Exact Total`, `Unavailable`
  - 세션 행: `Current <session-id>`, `Other <session-id>`

- 참고:
  - 폴링 주기는 5초(`5000ms`)입니다.
  - exact RAM 합계는 `mappingState=exact` 행만 합산합니다.
  - 검증 로그/증적은 이 저장소의 `.sisyphus/evidence/` 아래에 기록되어 있습니다.

### Japanese

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

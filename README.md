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

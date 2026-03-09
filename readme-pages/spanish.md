# Español

- [Back to Home](../README.md)
- [Language Index](index.md)

Languages: [English](english.md) | [中文](chinese.md) | [Español](spanish.md) | [한국어](korean.md) | [日本語](japanese.md)

## Installation

- Clona el repositorio y compila el binario:

```bash
git clone https://github.com/Topabaem05/CancerBroker.git
cd CancerBroker
cargo build --release
```

- Instala el plugin OpenCode Session Memory sin clonar el repositorio:

```bash
curl -fsSL https://raw.githubusercontent.com/Topabaem05/CancerBroker/main/install/opencode-session-memory-sidebar.sh | sh
opencode --restart
```

- Instala con Homebrew:

```bash
brew install topabaem05/cancerbroker/opencode-session-memory-sidebar-installer
opencode --restart
```

- En OpenCode 1.2.22 se carga como la herramienta personalizada compatible `session_memory`, no como un panel lateral personalizado.

- Si Homebrew necesita una URL de tap explicita:

```bash
brew tap topabaem05/cancerbroker https://github.com/Topabaem05/CancerBroker
brew install topabaem05/cancerbroker/opencode-session-memory-sidebar-installer
opencode --restart
```

## Usage

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

## Verification

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

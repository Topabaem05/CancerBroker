# Español

- [Back to Home](../README.md)
- [Language Index](index.md)

Languages: [English](english.md) | [中文](chinese.md) | [Español](spanish.md) | [한국어](korean.md) | [日本語](japanese.md)

## Installation

- Instala desde Git:

```bash
cargo install --git https://github.com/Topabaem05/CancerBroker.git
```

## Configuracion de Opencode

```bash
cancerbroker setup
```

## Usage

- Verifica el modo actual:

```bash
cancerbroker --config fixtures/config/observe-only.toml status --json
```

- Ejecuta una evaluacion de politica y escribe evidencia en `.sisyphus/evidence`:

```bash
cancerbroker --config fixtures/config/observe-only.toml run-once --json
```

- Inicia el daemon de limpieza de completion en ejecucion continua:

```bash
cancerbroker --config fixtures/config/completion-cleanup.toml daemon --json --max-events 128
```

## Prueba sandbox de terminacion de PID

```bash
cargo test --workspace run_leak_enforcement_with_inventory_terminates_leaking_process_in_enforce_mode -- --nocapture
```

Interpretacion de senales:

- `signal: 15` -> termina con `SIGTERM`
- `signal: 9` -> escala a `SIGKILL` despues de `SIGTERM`

## Verification

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
cargo build --workspace
```

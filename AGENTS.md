# AGENTS.md
Repository guide for autonomous coding agents working in `cancerbroker` / `opencode-guardian`.

## Repo Shape
- Primary app: Rust crate at the repository root.
- Secondary packages: npm/OpenCode packaging under `packaging/npm/`.
- Main runtime code: `src/`.
- Fixture configs and sample inputs: `fixtures/`.
- CI authority: `.github/workflows/ci.yml` and `.github/workflows/npm-publish.yml`.

## Important Facts
- Root crate name is `opencode-guardian` (`Cargo.toml`).
- Rust edition is `2024`.
- Rust tests are mostly inline unit tests in the same file as the implementation.
- No existing `.cursorrules` file was found.
- No files were found under `.cursor/rules/`.
- No `.github/copilot-instructions.md` file was found.

## Search Priorities
- Search `src/` first for product logic.
- Search `fixtures/config/` for real config examples.
- Search `packaging/npm/opencode-session-memory-sidebar` for plugin changes.
- Search `packaging/npm/opencode-session-memory-sidebar-installer` for installer changes.

## Search Exclusions
- Ignore `target/`.
- Ignore `packaging/**/node_modules/`; this repo contains vendored dependency trees.
- Ignore `.ruff_cache/`.

## Root Commands
Run these from `/Users/guribbong/code/cancerbroker`.

### Build
- Fast local build: `cargo build`
- Release build: `cargo build --release`

### Run
- Status: `cargo run -- --config fixtures/config/observe-only.toml status --json`
- One-shot evaluation: `cargo run -- --config fixtures/config/observe-only.toml run-once --json`
- Completion daemon: `cargo run -- --config fixtures/config/completion-cleanup.toml daemon --json --max-events 128`

### Format / Lint / Test
- Format check: `cargo fmt --all -- --check`
- Lint: `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`
- Full test suite: `cargo test --workspace --locked`
- List tests: `cargo test --workspace -- --list`

These commands match CI in `.github/workflows/ci.yml`.

### Single Rust Test
- Exact or substring match works: `cargo test cli::tests::status_output_renders_human_and_json_modes`
- Another example: `cargo test load_config_reports_missing_files`
- Because tests are inline module tests, target the Rust test path, not a file path.

## npm Packaging Commands

### Plugin Package
Working directory: `packaging/npm/opencode-session-memory-sidebar`

- Install deps: `bun install`
- Typecheck: `bunx tsc --noEmit -p tsconfig.json`
- Smoke import: `bun -e "import plugin from './src/index.ts'; console.log(typeof plugin)"`
- Package test script: `bun test`

Current repo fact: `bun test` is defined, but the package currently has no `*.test.*` or `*.spec.*` files, so it errors with `0 test files matching ...`.

If tests are added later, Bun single-test patterns are:
- One file: `bun test path/to/file.test.ts`
- One test name: `bun test --test-name-pattern "name fragment"`

### Installer Package
Working directory: `packaging/npm/opencode-session-memory-sidebar-installer`

- Install deps: `bun install`
- Local install flow: `node ./bin/install.mjs`
- Local uninstall flow: `node ./bin/uninstall.mjs`
- Smoke test: `TMP_DIR="$(mktemp -d)" && OPENCODE_CONFIG_DIR="$TMP_DIR" node ./bin/install.mjs && OPENCODE_CONFIG_DIR="$TMP_DIR" node ./bin/uninstall.mjs`
- Prepare next installer release: `node ./scripts/prepare-installer-release.mjs 0.1.1`

### Publish Workflow Facts
- Workflow file: `.github/workflows/npm-publish.yml`
- Release asset workflow: `.github/workflows/release-installer-asset.yml`
- Validation runs plugin checks before installer checks.
- Publish order is plugin package first, installer package second.
- Validation includes plugin typecheck and installer smoke tests.

## Rust Style Guide

### Imports
- Group imports as: standard library, external crates, then `crate::...` imports.
- Separate those groups with a blank line.
- Prefer explicit imports over glob imports.

### Formatting
- Follow `rustfmt` defaults.
- Use trailing commas in multiline literals and argument lists.
- Prefer small helpers over long monolithic functions.

### Types and Data Modeling
- Prefer concrete structs and enums over loose maps.
- Config/evidence structs commonly derive `Debug, Clone, PartialEq, Eq, Serialize, Deserialize`.
- Output structs derive only what they need, often just `Serialize`.
- Prefer typed domain fields like `PathBuf`, `BTreeMap`, enums, and new structs.

### Naming
- Modules, functions, and fields use `snake_case`.
- Types and enums use `PascalCase`.
- Constants use `SCREAMING_SNAKE_CASE`.
- Stable string codes are lowercase snake_case, for example `signal_quorum_not_met`, `cooldown_or_budget_active`, and `warn_throttle`.

### Error Handling
- Use dedicated error enums with `thiserror::Error` in library/domain modules.
- Keep error text specific and stable; many strings are asserted in tests.
- Convert lower-level errors with `map_err(...)` instead of panicking.
- At the CLI boundary, add context with `color_eyre` and `WrapErr`.
- Preserve non-destructive fallback behavior when writes or actions fail.

### Control Flow
- Prefer pure helpers like `build_policy_decision`, `count_recent_actions`, and `unix_timestamp_secs`.
- Prefer early returns over nested branches.
- Keep platform-specific behavior behind `#[cfg(unix)]` / `#[cfg(not(unix))]` splits.
- Do not loosen cleanup/remediation behavior casually; the codebase is intentionally conservative.

### Testing
- Keep tests in the same file under `#[cfg(test)] mod tests`.
- Use `assert_eq!`, `assert!`, and `matches!` heavily.
- Use `.expect("specific message")` in tests for setup clarity.
- Filesystem tests usually use `tempfile::tempdir()`.
- Async tests use `#[tokio::test]` only where required.

## TypeScript / Node Style Guide
Applies mainly to `packaging/npm/opencode-session-memory-sidebar` and installer scripts.

### Imports and Modules
- Use ESM syntax.
- Prefer `import type` for type-only imports.
- Use relative imports inside each package.
- In installer scripts, import Node built-ins from `node:*` specifiers.

### Formatting
- Use 2-space indentation.
- Use semicolons.
- Use double quotes.
- Prefer multiline object/type literals with trailing commas.

### Types
- `strict` TypeScript is enabled; keep it enabled.
- Do not introduce `any`.
- Prefer `unknown` at runtime boundaries and narrow with guards.
- Interface properties are usually `readonly`.
- Prefer discriminated unions for runtime state, especially `state` and `mappingState`.

### Naming
- Functions and locals use `camelCase`.
- Interfaces and type aliases use `PascalCase`.
- Exported constants use `SCREAMING_SNAKE_CASE` when truly constant.
- Keep domain wording stable: `sessionId`, `sampledAtIso`, `mappingState`, `projectPath`.

### Runtime Validation and Error Handling
- Normalize external data defensively with helpers like `asString`, `isRecord`, `unwrapSessionArray`, and `normalizeStatus`.
- Fallback probing sometimes uses `try/catch` with `continue` while checking multiple API shapes; only use that pattern for compatibility probing.
- Installer/config code should fail loudly on invalid config parse instead of mutating bad input.
- Preserve formatting and file safety when rewriting config files.

## Project-Specific Constraints
- The guardian defaults to observe-first behavior.
- Respect allowlists and same-UID checks when touching cleanup/remediation paths.
- Do not broaden destructive behavior casually.
- Keep JSON output shapes stable when tests assert exact payloads.
- Keep the installer default package target aligned with the actual plugin package name.

## Recommended Agent Workflow
- After Rust edits: run `cargo fmt --all -- --check`, targeted `cargo test`, and usually `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`.
- After plugin edits: run `bunx tsc --noEmit -p tsconfig.json` and the smoke import.
- After installer edits: run the temp-dir install/uninstall smoke test.
- Before tagging a new installer release: run `node ./scripts/prepare-installer-release.mjs <version>` and verify the rebuilt standalone asset.
- Prefer repository patterns over generic framework advice.
- Prefer extending existing helpers over adding new abstractions.
- Prefer inline unit tests over new harnesses unless cross-module coverage is truly required.

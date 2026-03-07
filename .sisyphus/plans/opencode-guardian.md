# opencode-guardian

## TL;DR

> **Quick Summary**: Build `opencode-guardian` as a Rust sidecar watchdog that protects `opencode` plus `oh-my-openagent` from memory-leak amplification, runaway subagent trees, and stale session storage by reconciling process and storage state externally rather than trusting upstream cleanup events alone.
>
> **Deliverables**:
> - A Unix-first (`macOS` + `Linux`) Rust daemon/CLI with TDD-first safety invariants
> - Process-tree + storage monitors, policy engine, evidence capture, and conservative remediation
> - Local-only metrics/logging, sample service-manager manifests, and optional phase-2 IPC/plugin hooks
>
> **Estimated Effort**: Large
> **Parallel Execution**: YES - 6 waves
> **Critical Path**: Task 0 -> Task 1 -> Task 3 -> Task 5 -> Task 6 -> Task 8

---

## Context

### Original Request
Plan a root-cause-driven fix strategy for `opencode` plus `oh-my-opencode` / `oh-my-openagent` memory leaks using a Rust-based watchdog sidecar, with exhaustive validation against current public code, issues, PRs, and architecture patterns.

### Interview Summary

**Key Discussions**:
- External sidecar is the preferred v1 boundary; upstream patches are not assumed.
- `TDD` is required, and agent-executed QA is mandatory in addition to automated tests.
- v1 platform scope is `macOS + Linux`; Windows is explicitly deferred.
- Safety posture is `Safety-first`: evidence first, staged remediation, conservative cleanup, observe-only default.
- Storage cleanup boundary is conservative: stale `session` / `message` / `session_diff` style artifacts only, never broader project state by default.

**Research Findings**:
- `oh-my-opencode` is now `oh-my-openagent`; several public references still use the old name.
- `opencode` app/web reducers already clean `session` caches on `session.deleted`, but TUI sync still leaves orphaned keyed data, matching public leak reports.
- `oh-my-openagent` still relies on multiple Sets/Maps/timers in background-task/session management; cleanup exists but is selective and event-dependent.
- Public issues and PRs confirm recurring leak classes: orphaned session state, TUI cache retention, event-stream disposal gaps, storage accumulation, and unbounded output growth.

### Metis Review

**Identified Gaps** (addressed in this plan):
- Add explicit v1 non-goals: no Windows, no remote control backend, no generic app-supervisor scope, no broad storage vacuuming.
- Add explicit safety invariants: same-UID ownership checks, canonical-path allowlists, active-session grace windows, cooldowns, and action budgets.
- Add explicit negative tests for false-positive prevention and cleanup boundaries.
- Add edge-case coverage for PID reuse, watcher overflow, sleep/wake clock jumps, concurrent guardians, corrupted artifacts, and schema drift.

---

## Work Objectives

### Core Objective
Create a single Rust project that can observe, diagnose, and conservatively mitigate the most credible `opencode` / `oh-my-openagent` leak and runaway scenarios without requiring in-process modifications.

### Concrete Deliverables
- `opencode-guardian` Rust workspace with CLI/daemon entrypoint and fixture-driven test suite
- Config schema for thresholds, allowlists, evidence retention, cooldowns, and mode (`observe` vs `enforce`)
- Process monitor, storage monitor, policy engine, evidence store, remediation executor, and reconciliation loop
- Local-only logs/metrics and sample deployment artifacts for `launchd` / `systemd`
- Optional phase-2 local IPC/plugin integration hooks that are feature-gated and not required for core protection

### Definition of Done
- [ ] `cargo fmt --all -- --check` exits `0`
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings` exits `0`
- [ ] `cargo test --workspace` reports `test result: ok`
- [ ] Observe-only E2E fixture captures evidence without killing or deleting anything
- [ ] Enforce-mode E2E fixture terminates only validated owned targets and only deletes allowlisted stale artifacts

### Must Have
- External, Unix-first sidecar with no required upstream patches
- Observe-only default on first run, with explicit opt-in for destructive enforcement
- Multi-signal + time-window gating before any remediation
- Evidence capture before destructive action, with redacted metadata-first audit output
- TDD coverage for negative safety cases, race conditions, and recovery paths

### Must NOT Have (Guardrails)
- No Windows support in v1
- No network egress or remote telemetry backend by default
- No deletion outside canonicalized allowlisted session artifact paths
- No destructive action on processes not proven to belong to the same user and expected lineage
- No dependence on `session.deleted` or any single upstream event as the only cleanup signal
- No generic "supervise arbitrary applications" scope creep in v1

### Default Operating Policy
- Sampling interval: every `5s`
- Breach quorum: at least `2` independent signals over `3` of the last `5` samples before destructive eligibility
- Soft alert defaults: RSS slope `>= 200 MiB/min` for `5 min`, orphaned task/process count `>= 3`, or allowlisted stale-artifact growth `>= 5 GiB`
- Hard remediation eligibility: soft-breach quorum plus verified same-UID target ownership and successful evidence capture
- Active-session grace window: `10 min`
- Destructive action budget: max `1` destructive remediation per target per `60 min`, max `3` per `24 h`
- Evidence retention default: `7 days` or `500 MiB`, whichever limit is hit first
- Metrics binding: `127.0.0.1` only

---

## Verification Strategy (MANDATORY)

> **UNIVERSAL RULE: ZERO HUMAN INTERVENTION**
>
> Every task must be verifiable without manual testing. The executing agent runs commands, inspects JSON/log output, checks filesystem state, and captures evidence automatically.

### Test Decision
- **Infrastructure exists**: NO
- **Automated tests**: TDD
- **Framework**: `cargo test` + fixture-based integration tests

### If TDD Enabled

Each task follows `RED -> GREEN -> REFACTOR`:
1. **RED**: add/extend failing unit or integration tests first
2. **GREEN**: implement the minimum code to satisfy the new tests
3. **REFACTOR**: simplify without changing observable behavior

**Test Setup Task**:
- Bootstrap the workspace, fixture directories, CI jobs, lint targets, and helper test binaries/scripts before writing core remediation logic.

### Agent-Executed QA Scenarios (MANDATORY)

Use these tool classes throughout the plan:
- **Bash**: build the daemon, run fixture scenarios, inspect JSON/log/evidence output
- **interactive_bash**: only for long-lived daemon sessions that need signal delivery or controlled shutdown

Each task below includes named scenarios with:
- exact command or binary invocation
- exact fixture path
- exact assertions on logs, JSON, or filesystem state
- exact evidence path under `.sisyphus/evidence/`

---

## Execution Strategy

### Parallel Execution Waves

```text
Wave 1:
- Task 0

Wave 2:
- Task 1
- Task 2

Wave 3:
- Task 3
- Task 4

Wave 4:
- Task 5

Wave 5:
- Task 6

Wave 6:
- Task 7

Wave 7:
- Task 8

Critical Path: 0 -> 1 -> 3 -> 5 -> 6 -> 8
```

### Dependency Matrix

| Task | Depends On | Blocks | Can Parallelize With |
|------|------------|--------|----------------------|
| 0 | None | 1, 2 | None |
| 1 | 0 | 3, 4, 5 | 2 |
| 2 | 0 | 3, 4, 6 | 1 |
| 3 | 1, 2 | 5, 6 | 4 |
| 4 | 1, 2 | 5, 6 | 3 |
| 5 | 3, 4 | 6, 8 | None |
| 6 | 2, 3, 4, 5 | 7, 8 | None |
| 7 | 6 | 8 | None |
| 8 | 6, 7 | None | None |

### Agent Dispatch Summary

| Wave | Tasks | Recommended Agents |
|------|-------|-------------------|
| 1 | 0 | `task(category="unspecified-high", load_skills=[], run_in_background=false)` |
| 2 | 1, 2 | parallel dispatch after Wave 1 |
| 3 | 3, 4 | parallel dispatch after Wave 2 |
| 4 | 5 | sequential safety-critical executor task |
| 5 | 6 | integration task |
| 6 | 7 | phase-2 integration task |
| 7 | 8 | final hardening and release task |

---

## TODOs

- [x] 0. Bootstrap the Rust workspace and TDD harness

  **What to do**:
  - Create the `opencode-guardian` Cargo workspace and binary crate.
  - Add core dependencies, fixture directories, lint targets, and CI matrix for `macOS` + `Linux`.
  - Create baseline test modules for config parsing, policy fixtures, process fixtures, and E2E fixture orchestration.

  **Must NOT do**:
  - Do not implement destructive remediation logic yet.
  - Do not include Windows-only code paths.
  - Do not add remote telemetry or external service dependencies.

  **Recommended Agent Profile**:
  - **Category**: `unspecified-high`
    - Reason: multi-file project bootstrap with infra and test scaffolding.
  - **Skills**: `none`
    - Reason: no listed built-in skill materially overlaps with Rust daemon bootstrap.
  - **Skills Evaluated but Omitted**:
    - `playwright`: no browser surface in this task.
    - `git-master`: implementation task, not repository history work.

  **Parallelization**:
  - **Can Run In Parallel**: NO
  - **Parallel Group**: Wave 1
  - **Blocks**: 1, 2
  - **Blocked By**: None

  **References**:
  - `https://tokio.rs/tokio/topics/shutdown` - Tokio lifecycle and shutdown structure to mirror from the start.
  - `https://docs.rs/sysinfo/latest/sysinfo/` - process-monitor dependency API surface to wire into the workspace.
  - `https://docs.rs/notify/latest/notify/` - watcher dependency expectations and platform caveats.
  - `https://github.com/anomalyco/opencode/issues/9385` - reminder that the project exists to harden against large real-world leaks, not as a generic daemon.

  **Acceptance Criteria**:
  - [ ] `cargo fmt --all -- --check` -> exit code `0`
  - [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings` -> exit code `0`
  - [ ] `cargo test --workspace` -> output contains `test result: ok`
  - [ ] `cargo test -p opencode-guardian --test bootstrap -- --exact parses_minimal_observe_config` -> output contains `... ok`

  **Agent-Executed QA Scenarios**:

  ```text
  Scenario: Minimal config boots the CLI in observe mode
    Tool: Bash
    Preconditions: Workspace created; fixture file at fixtures/config/observe-only.toml
    Steps:
      1. Run: cargo run -p opencode-guardian -- --config fixtures/config/observe-only.toml status --json
      2. Assert: exit code is 0
      3. Assert: stdout JSON contains "mode":"observe"
      4. Capture: .sisyphus/evidence/task-0-status.json
    Expected Result: CLI reports valid observe-only status without starting remediation
    Evidence: .sisyphus/evidence/task-0-status.json

  Scenario: Invalid config fails safely
    Tool: Bash
    Preconditions: Fixture file at fixtures/config/invalid.toml
    Steps:
      1. Run: cargo run -p opencode-guardian -- --config fixtures/config/invalid.toml status --json
      2. Assert: exit code is non-zero
      3. Assert: stderr contains "config"
      4. Capture: .sisyphus/evidence/task-0-invalid-config.txt
    Expected Result: CLI refuses to start and explains the config error
    Evidence: .sisyphus/evidence/task-0-invalid-config.txt
  ```

  **Commit**: YES
  - Message: `build(guardian): bootstrap workspace and test harness`
  - Files: `Cargo.toml`, `src/`, `tests/`, `fixtures/`, `.github/workflows/`
  - Pre-commit: `cargo test --workspace`

- [ ] 1. Define the safety contract, config schema, and evidence model

  **What to do**:
  - Define config types for thresholds, cooldowns, action budgets, artifact allowlists, evidence retention, and mode selection.
  - Define canonical-path, same-UID, and parent-lineage invariants as reusable helpers.
  - Define a redacted evidence schema for decision records, breach windows, and remediation outcomes.

  **Must NOT do**:
  - Do not log prompt contents, environment secrets, or unrestricted file paths.
  - Do not permit broad cleanup globs or unchecked symlink traversal.

  **Recommended Agent Profile**:
  - **Category**: `ultrabrain`
    - Reason: this is the core safety contract that governs all later destructive behavior.
  - **Skills**: `none`
    - Reason: available built-in skills do not cover Rust safety-policy design.
  - **Skills Evaluated but Omitted**:
    - `dev-browser`: no browser interaction needed.

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 2 (with Task 2)
  - **Blocks**: 3, 4, 5
  - **Blocked By**: 0

  **References**:
  - `https://github.com/anomalyco/opencode/issues/4980#issuecomment-3602766335` - maintainer confirmation that storage does not auto-prune, which justifies explicit retention config.
  - `https://github.com/anomalyco/opencode/issues/9157` - plugin/resource disposal gap motivating conservative ownership and cleanup rules.
  - `https://docs.rs/config/latest/config/` - layered config loading and validation approach.
  - `https://docs.rs/tracing/latest/tracing/` - structured event model for redacted evidence records.

  **Acceptance Criteria**:
  - [ ] `cargo test -p opencode-guardian --test config_contract -- --exact parses_thresholds_and_budgets` -> `... ok`
  - [ ] `cargo test -p opencode-guardian --test process_safety -- --exact never_remediate_non_owned_process` -> `... ok`
  - [ ] `cargo test -p opencode-guardian --test storage_boundary -- --exact rejects_non_allowlisted_paths` -> `... ok`
  - [ ] `cargo test -p opencode-guardian --test evidence_schema -- --exact redacts_sensitive_fields` -> `... ok`

  **Agent-Executed QA Scenarios**:

  ```text
  Scenario: Allowlisted artifact path is accepted and normalized
    Tool: Bash
    Preconditions: Fixture config at fixtures/config/allowlist-only.toml
    Steps:
      1. Run: cargo test -p opencode-guardian --test storage_boundary -- --exact rejects_non_allowlisted_paths
      2. Assert: exit code is 0
      3. Capture: .sisyphus/evidence/task-1-storage-boundary.txt
    Expected Result: only canonicalized approved paths pass the contract
    Evidence: .sisyphus/evidence/task-1-storage-boundary.txt

  Scenario: Evidence serialization redacts sensitive fields
    Tool: Bash
    Preconditions: Fixture evidence sample exists in tests/fixtures/evidence/
    Steps:
      1. Run: cargo test -p opencode-guardian --test evidence_schema -- --exact redacts_sensitive_fields
      2. Assert: exit code is 0
      3. Assert: output does not contain raw prompt text or env keys
      4. Capture: .sisyphus/evidence/task-1-evidence-redaction.txt
    Expected Result: evidence records remain operator-useful without leaking sensitive content
    Evidence: .sisyphus/evidence/task-1-evidence-redaction.txt
  ```

  **Commit**: YES
  - Message: `feat(guardian): define safety contract and config schema`
  - Files: `src/config.rs`, `src/evidence.rs`, `src/safety.rs`, `tests/`
  - Pre-commit: `cargo test -p opencode-guardian --test process_safety`

- [ ] 2. Implement process and storage inventory monitors

  **What to do**:
  - Build process sampling around `sysinfo` with parent-PID indexing rather than assuming a `children()` helper.
  - Build storage scanning plus `notify`-based watching with periodic reconciliation fallback.
  - Track memory slope, CPU, process fanout, artifact ages, and scan metadata needed by the policy engine.

  **Must NOT do**:
  - Do not kill or delete anything.
  - Do not trust watcher events as exhaustive; periodic scans remain mandatory.
  - Do not couple monitor output to a specific upstream version string.

  **Recommended Agent Profile**:
  - **Category**: `unspecified-high`
    - Reason: systems-integration task with platform APIs and fixture-heavy testing.
  - **Skills**: `none`
    - Reason: no available skill directly overlaps.
  - **Skills Evaluated but Omitted**:
    - `playwright`: irrelevant to process and filesystem monitoring.

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 2 (with Task 1)
  - **Blocks**: 3, 4, 6
  - **Blocked By**: 0

  **References**:
  - `https://docs.rs/sysinfo/latest/sysinfo/` - process refresh and sampling APIs.
  - `https://github.com/openai/codex/blob/main/codex-rs/utils/pty/src/process_group.rs` - real-world process-group handling pattern.
  - `https://docs.rs/notify/latest/notify/` - watcher semantics and known caveats.
  - `https://github.com/watchexec/watchexec/blob/main/crates/lib/src/sources/fs.rs` - pattern for watcher + fallback scan coexistence.
  - `https://github.com/anomalyco/opencode/issues/5734` - stale session/storage accumulation signal source.

  **Acceptance Criteria**:
  - [ ] `cargo test -p opencode-guardian --test process_inventory -- --exact indexes_process_tree_by_parent_pid` -> `... ok`
  - [ ] `cargo test -p opencode-guardian --test process_inventory -- --exact pid_reuse_does_not_match_old_process` -> `... ok`
  - [ ] `cargo test -p opencode-guardian --test storage_inventory -- --exact periodic_scan_recovers_after_missed_watch_event` -> `... ok`
  - [ ] `cargo test -p opencode-guardian --test storage_inventory -- --exact ignores_non_allowlisted_roots` -> `... ok`

  **Agent-Executed QA Scenarios**:

  ```text
  Scenario: Monitor reconstructs a fake opencode process tree
    Tool: Bash
    Preconditions: Fixture helper spawns parent/child processes under tests/fixtures/process-tree/
    Steps:
      1. Run: cargo test -p opencode-guardian --test process_inventory -- --exact indexes_process_tree_by_parent_pid
      2. Assert: exit code is 0
      3. Capture: .sisyphus/evidence/task-2-process-tree.txt
    Expected Result: monitor reports the expected parent-child lineage without using broad heuristics
    Evidence: .sisyphus/evidence/task-2-process-tree.txt

  Scenario: Missed watcher event is recovered by periodic reconciliation
    Tool: Bash
    Preconditions: Storage fixtures under tests/fixtures/storage/
    Steps:
      1. Run: cargo test -p opencode-guardian --test storage_inventory -- --exact periodic_scan_recovers_after_missed_watch_event
      2. Assert: exit code is 0
      3. Capture: .sisyphus/evidence/task-2-watch-recovery.txt
    Expected Result: stale artifacts are rediscovered even if the watcher stream misses an event
    Evidence: .sisyphus/evidence/task-2-watch-recovery.txt
  ```

  **Commit**: YES
  - Message: `feat(guardian): add process and storage monitors`
  - Files: `src/monitor/`, `tests/process_inventory.rs`, `tests/storage_inventory.rs`
  - Pre-commit: `cargo test -p opencode-guardian --test process_inventory`

- [ ] 3. Implement the policy engine and remediation ladder

  **What to do**:
  - Convert monitor signals into time-windowed breaches, quorum decisions, and action candidates.
  - Implement observe-only, staged escalation, cooldowns, and per-target action budgets.
  - Make all thresholds config-driven and persist decision records before action execution.

  **Must NOT do**:
  - Do not allow single-signal destructive action.
  - Do not embed thresholds only in code with no config override.
  - Do not escalate past stage 0 in observe-only mode.

  **Recommended Agent Profile**:
  - **Category**: `ultrabrain`
    - Reason: logic-heavy policy and safety engine with high false-positive risk.
  - **Skills**: `none`
    - Reason: no built-in skill materially overlaps with this policy logic.
  - **Skills Evaluated but Omitted**:
    - `frontend-ui-ux`: no UI surface.

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 3 (with Task 4)
  - **Blocks**: 5, 6
  - **Blocked By**: 1, 2

  **References**:
  - `https://github.com/anomalyco/opencode/issues/10913` - long-running session leak classes that require multi-signal detection.
  - `https://github.com/anomalyco/opencode/issues/9743` - severe OOM symptom profile motivating action budgets and escalation.
  - `https://github.com/code-yeongyu/oh-my-openagent/issues/1222` - background subagent accumulation as a motivating signal source.
  - `https://tokio.rs/tokio/topics/shutdown` - staged shutdown thinking for escalation design.

  **Acceptance Criteria**:
  - [ ] `cargo test -p opencode-guardian --test remediation_ladder -- --exact single_signal_no_action` -> `... ok`
  - [ ] `cargo test -p opencode-guardian --test remediation_ladder -- --exact multisignal_triggers_step1_only` -> `... ok`
  - [ ] `cargo test -p opencode-guardian --test remediation_ladder -- --exact cooldown_blocks_repeat_action` -> `... ok`
  - [ ] `cargo test -p opencode-guardian --test remediation_ladder -- --exact observe_mode_records_without_action` -> `... ok`

  **Agent-Executed QA Scenarios**:

  ```text
  Scenario: Weak evidence produces no remediation
    Tool: Bash
    Preconditions: Fixture snapshot under tests/fixtures/policy/weak-evidence.json
    Steps:
      1. Run: cargo test -p opencode-guardian --test remediation_ladder -- --exact single_signal_no_action
      2. Assert: exit code is 0
      3. Capture: .sisyphus/evidence/task-3-single-signal.txt
    Expected Result: policy records the breach candidate but schedules no kill or delete action
    Evidence: .sisyphus/evidence/task-3-single-signal.txt

  Scenario: Observe mode captures rationale without acting
    Tool: Bash
    Preconditions: Fixture config forces observe mode
    Steps:
      1. Run: cargo test -p opencode-guardian --test remediation_ladder -- --exact observe_mode_records_without_action
      2. Assert: exit code is 0
      3. Assert: output contains "proposed_action"
      4. Assert: output does not contain "executed_action"
      5. Capture: .sisyphus/evidence/task-3-observe-mode.txt
    Expected Result: operator gets decision evidence without any destructive effect
    Evidence: .sisyphus/evidence/task-3-observe-mode.txt
  ```

  **Commit**: YES
  - Message: `feat(guardian): add policy engine and remediation ladder`
  - Files: `src/policy.rs`, `tests/remediation_ladder.rs`
  - Pre-commit: `cargo test -p opencode-guardian --test remediation_ladder`

- [ ] 4. Implement evidence capture and audit persistence

  **What to do**:
  - Persist pre-action evidence bundles with timestamps, signal series, target identity proof, rationale, and redacted log excerpts.
  - Add retention and pruning for evidence bundles themselves.
  - Ensure evidence capture failure downgrades behavior to non-destructive action.

  **Must NOT do**:
  - Do not make evidence optional before kill/delete stages.
  - Do not store raw prompt bodies or unrestricted env dumps.
  - Do not expose metrics outside localhost.

  **Recommended Agent Profile**:
  - **Category**: `unspecified-high`
    - Reason: cross-cutting persistence and audit design with privacy constraints.
  - **Skills**: `none`
    - Reason: no available skill maps directly.
  - **Skills Evaluated but Omitted**:
    - `dev-browser`: not relevant.

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 3 (with Task 3)
  - **Blocks**: 5, 6
  - **Blocked By**: 1, 2

  **References**:
  - `https://docs.rs/tracing/latest/tracing/` - structured event capture model.
  - `https://docs.rs/tracing-subscriber/latest/tracing_subscriber/` - layered logging and local sinks.
  - `https://docs.rs/metrics/latest/metrics/` - metric naming and recording conventions.
  - `https://github.com/anomalyco/opencode/issues/13776` - example of resource-lifecycle leak where evidence needs to show stream/state context.

  **Acceptance Criteria**:
  - [ ] `cargo test -p opencode-guardian --test evidence_schema -- --exact captures_required_pre_action_fields` -> `... ok`
  - [ ] `cargo test -p opencode-guardian --test evidence_schema -- --exact redacts_sensitive_fields` -> `... ok`
  - [ ] `cargo test -p opencode-guardian --test evidence_schema -- --exact evidence_failure_forces_non_destructive_fallback` -> `... ok`

  **Agent-Executed QA Scenarios**:

  ```text
  Scenario: Pre-action evidence bundle is written before escalation
    Tool: Bash
    Preconditions: Fixture breach record under tests/fixtures/evidence/breach.json
    Steps:
      1. Run: cargo test -p opencode-guardian --test evidence_schema -- --exact captures_required_pre_action_fields
      2. Assert: exit code is 0
      3. Assert: output references a JSON evidence artifact
      4. Capture: .sisyphus/evidence/task-4-preaction.txt
    Expected Result: the required decision record exists before any executor can run
    Evidence: .sisyphus/evidence/task-4-preaction.txt

  Scenario: Evidence failure blocks destructive escalation
    Tool: Bash
    Preconditions: Fixture simulates unwritable evidence directory
    Steps:
      1. Run: cargo test -p opencode-guardian --test evidence_schema -- --exact evidence_failure_forces_non_destructive_fallback
      2. Assert: exit code is 0
      3. Assert: output contains "fallback"
      4. Capture: .sisyphus/evidence/task-4-fallback.txt
    Expected Result: policy downgrades to observe-only/alert behavior when evidence cannot be captured safely
    Evidence: .sisyphus/evidence/task-4-fallback.txt
  ```

  **Commit**: YES
  - Message: `feat(guardian): add evidence capture and audit persistence`
  - Files: `src/evidence.rs`, `tests/evidence_schema.rs`
  - Pre-commit: `cargo test -p opencode-guardian --test evidence_schema`

- [ ] 5. Implement safe process remediation and conservative storage cleanup executors

  **What to do**:
  - Implement same-UID process validation, graceful `TERM`, timeout, then `KILL` for verified owned targets.
  - Implement conservative stale-artifact cleanup for allowlisted session/message/session_diff paths only.
  - Add action budgets, active-session grace windows, and race-safe checks before each delete.

  **Must NOT do**:
  - Do not touch `project` or broader workspace state by default.
  - Do not kill processes without identity proof (PID + start time + lineage + same UID).
  - Do not delete artifacts still inside the active-session grace window.

  **Recommended Agent Profile**:
  - **Category**: `ultrabrain`
    - Reason: highest-risk task; mistakes cause data loss or false-positive termination.
  - **Skills**: `none`
    - Reason: built-in skills do not cover Rust systems remediation.
  - **Skills Evaluated but Omitted**:
    - `git-master`: unrelated to runtime safety logic.

  **Parallelization**:
  - **Can Run In Parallel**: NO
  - **Parallel Group**: Wave 4
  - **Blocks**: 6, 8
  - **Blocked By**: 3, 4

  **References**:
  - `https://docs.rs/sysinfo/latest/sysinfo/` - `kill_with` and process identity APIs.
  - `https://github.com/openai/codex/blob/main/codex-rs/utils/pty/src/process_group.rs` - process-group termination pattern to adapt safely.
  - `https://github.com/code-yeongyu/oh-my-openagent/pull/1243` - zombie/shutdown cleanup precedent motivating deterministic descendant cleanup.
  - `https://github.com/anomalyco/opencode/issues/12218` - orphaned session cache cleanup rationale.
  - `https://github.com/anomalyco/opencode/issues/12351` - keyed-state cleanup scope and leak symptoms.

  **Acceptance Criteria**:
  - [ ] `cargo test -p opencode-guardian --test process_safety -- --exact graceful_term_then_kill_hung_owned_process` -> `... ok`
  - [ ] `cargo test -p opencode-guardian --test process_safety -- --exact never_remediate_non_owned_process` -> `... ok`
  - [ ] `cargo test -p opencode-guardian --test storage_boundary -- --exact only_allowlisted_artifacts_removed` -> `... ok`
  - [ ] `cargo test -p opencode-guardian --test race_conditions -- --exact active_session_not_deleted_during_write` -> `... ok`

  **Agent-Executed QA Scenarios**:

  ```text
  Scenario: Hung owned fixture process gets TERM then KILL
    Tool: Bash
    Preconditions: Fixture helper spawns an owned process that ignores the first termination signal
    Steps:
      1. Run: cargo test -p opencode-guardian --test process_safety -- --exact graceful_term_then_kill_hung_owned_process
      2. Assert: exit code is 0
      3. Assert: output contains both "SIGTERM" and "SIGKILL"
      4. Capture: .sisyphus/evidence/task-5-process-remediation.txt
    Expected Result: executor performs staged termination only on validated targets
    Evidence: .sisyphus/evidence/task-5-process-remediation.txt

  Scenario: Cleanup never deletes out-of-bound state
    Tool: Bash
    Preconditions: Fixtures include allowlisted and non-allowlisted artifacts
    Steps:
      1. Run: cargo test -p opencode-guardian --test storage_boundary -- --exact only_allowlisted_artifacts_removed
      2. Assert: exit code is 0
      3. Assert: output confirms non-allowlisted fixtures remain present
      4. Capture: .sisyphus/evidence/task-5-storage-cleanup.txt
    Expected Result: only approved stale artifacts are removed
    Evidence: .sisyphus/evidence/task-5-storage-cleanup.txt
  ```

  **Commit**: YES
  - Message: `feat(guardian): add safe remediation and cleanup executors`
  - Files: `src/remediation.rs`, `src/cleanup.rs`, `tests/process_safety.rs`, `tests/storage_boundary.rs`
  - Pre-commit: `cargo test -p opencode-guardian --test process_safety`

- [ ] 6. Wire the reconciliation loop, observe-only daemon mode, CLI, and local metrics

  **What to do**:
  - Integrate monitors, policy engine, evidence store, and executors into the long-running control loop.
  - Add CLI commands such as `status`, `scan`, and `run` with local-only metrics/logging.
  - Expose health and decision summaries without enabling remote control.

  **Must NOT do**:
  - Do not bind metrics to non-local interfaces.
  - Do not enable destructive enforcement by default.
  - Do not require IPC/plugin integration for the daemon to be useful.

  **Recommended Agent Profile**:
  - **Category**: `unspecified-high`
    - Reason: integration-heavy daemon orchestration with operational surfaces.
  - **Skills**: `none`
    - Reason: no listed skill directly applies.
  - **Skills Evaluated but Omitted**:
    - `playwright`: no browser UI.

  **Parallelization**:
  - **Can Run In Parallel**: NO
  - **Parallel Group**: Wave 5
  - **Blocks**: 7, 8
  - **Blocked By**: 2, 3, 4, 5

  **References**:
  - `https://tokio.rs/tokio/topics/shutdown` - daemon lifecycle and shutdown orchestration.
  - `https://docs.rs/clap/latest/clap/` - CLI surface design.
  - `https://docs.rs/metrics-exporter-prometheus/latest/metrics_exporter_prometheus/` - localhost-only metrics exporter.
  - `https://docs.rs/tracing-subscriber/latest/tracing_subscriber/` - reloadable/local log sinks.

  **Acceptance Criteria**:
  - [ ] `cargo test -p opencode-guardian --test metrics -- --exact prometheus_endpoint_localhost_only` -> `... ok`
  - [ ] `cargo test -p opencode-guardian --test e2e_observe_mode -- --exact observe_mode_captures_evidence_without_action` -> `... ok`
  - [ ] `cargo run -p opencode-guardian -- --config fixtures/config/observe-only.toml status --json` -> exit code `0`

  **Agent-Executed QA Scenarios**:

  ```text
  Scenario: Observe-only daemon detects a breach and records evidence without acting
    Tool: Bash
    Preconditions: Fixture config at fixtures/config/observe-only.toml; fixture breach generator available
    Steps:
      1. Run: cargo test -p opencode-guardian --test e2e_observe_mode -- --exact observe_mode_captures_evidence_without_action
      2. Assert: exit code is 0
      3. Assert: output contains "proposed_action"
      4. Assert: output does not contain "executed_action"
      5. Capture: .sisyphus/evidence/task-6-observe-mode.txt
    Expected Result: full detection and evidence path works with zero destructive effect
    Evidence: .sisyphus/evidence/task-6-observe-mode.txt

  Scenario: Metrics endpoint stays local-only
    Tool: Bash
    Preconditions: Test fixture starts metrics exporter on a test port
    Steps:
      1. Run: cargo test -p opencode-guardian --test metrics -- --exact prometheus_endpoint_localhost_only
      2. Assert: exit code is 0
      3. Capture: .sisyphus/evidence/task-6-metrics.txt
    Expected Result: metrics are reachable on localhost and not exposed publicly
    Evidence: .sisyphus/evidence/task-6-metrics.txt
  ```

  **Commit**: YES
  - Message: `feat(guardian): integrate control loop and operational surfaces`
  - Files: `src/main.rs`, `src/daemon/`, `src/cli.rs`, `tests/e2e_observe_mode.rs`, `tests/metrics.rs`
  - Pre-commit: `cargo test -p opencode-guardian --test e2e_observe_mode`

- [ ] 7. Add feature-gated phase-2 IPC and service-manager integration artifacts

  **What to do**:
  - Add optional local IPC surface for future plugin commands, behind an explicit feature flag or mode.
  - Add sample `launchd` and `systemd` manifests/install docs for daemonized operation.
  - Keep IPC read-only or hint-oriented by default; it must never become the single source of truth.

  **Must NOT do**:
  - Do not make IPC or service-install support a blocker for the core protective loop.
  - Do not add remote control endpoints.
  - Do not enable IPC by default in the MVP runtime path.

  **Recommended Agent Profile**:
  - **Category**: `unspecified-high`
    - Reason: integration task with platform packaging implications.
  - **Skills**: `none`
    - Reason: no built-in skill covers Rust IPC/service-manager setup.
  - **Skills Evaluated but Omitted**:
    - `writing`: docs are part of the task, but the task is still primarily systems integration.

  **Parallelization**:
  - **Can Run In Parallel**: NO
  - **Parallel Group**: Wave 6
  - **Blocks**: 8
  - **Blocked By**: 6

  **References**:
  - `https://docs.rs/interprocess/latest/interprocess/` - cross-platform local IPC abstraction for future expansion.
  - `https://docs.rs/tokio/latest/tokio/net/struct.UnixListener.html` - Unix socket fallback/reference.
  - `https://github.com/code-yeongyu/oh-my-openagent/releases/tag/v3.9.0` - cleanup-related release context for plugin-facing future integration.

  **Acceptance Criteria**:
  - [ ] `cargo test -p opencode-guardian --test ipc_surface -- --exact ipc_disabled_by_default` -> `... ok`
  - [ ] `cargo test -p opencode-guardian --test ipc_surface -- --exact read_only_status_request_succeeds_when_enabled` -> `... ok`
  - [ ] `cargo test -p opencode-guardian --test packaging -- --exact systemd_and_launchd_samples_render_with_fixture_paths` -> `... ok`

  **Agent-Executed QA Scenarios**:

  ```text
  Scenario: IPC remains disabled unless explicitly enabled
    Tool: Bash
    Preconditions: Default config leaves IPC off
    Steps:
      1. Run: cargo test -p opencode-guardian --test ipc_surface -- --exact ipc_disabled_by_default
      2. Assert: exit code is 0
      3. Capture: .sisyphus/evidence/task-7-ipc-default.txt
    Expected Result: core daemon behavior is unchanged with IPC absent
    Evidence: .sisyphus/evidence/task-7-ipc-default.txt

  Scenario: Packaging samples render with fixture-specific paths
    Tool: Bash
    Preconditions: Fixture output paths under tests/fixtures/packaging/
    Steps:
      1. Run: cargo test -p opencode-guardian --test packaging -- --exact systemd_and_launchd_samples_render_with_fixture_paths
      2. Assert: exit code is 0
      3. Capture: .sisyphus/evidence/task-7-packaging.txt
    Expected Result: sample service definitions are internally consistent and path-safe
    Evidence: .sisyphus/evidence/task-7-packaging.txt
  ```

  **Commit**: YES
  - Message: `feat(guardian): add optional ipc and service integration artifacts`
  - Files: `src/ipc/`, `packaging/`, `tests/ipc_surface.rs`, `tests/packaging.rs`
  - Pre-commit: `cargo test -p opencode-guardian --test ipc_surface`

- [ ] 8. Run reproducer fixtures, soak tests, CI hardening, and release packaging

  **What to do**:
  - Build fixture scenarios that emulate opencode/OMO leak amplification patterns, including orphaned processes, stale storage, and long-running output growth.
  - Add soak tests and CI workflows for `ubuntu-latest` and `macos-latest`.
  - Document the supported compatibility envelope, non-goals, rollout sequence, and fallback-to-observe behavior.

  **Must NOT do**:
  - Do not claim compatibility with Windows or arbitrary upstream versions not exercised in fixtures.
  - Do not skip long-running safety scenarios.
  - Do not require human verification steps in CI or release validation.

  **Recommended Agent Profile**:
  - **Category**: `unspecified-high`
    - Reason: final integration, release hardening, and regression coverage.
  - **Skills**: `none`
    - Reason: no listed built-in skill directly overlaps.
  - **Skills Evaluated but Omitted**:
    - `git-master`: not relevant to verification design itself.

  **Parallelization**:
  - **Can Run In Parallel**: NO
  - **Parallel Group**: Wave 7
  - **Blocks**: None
  - **Blocked By**: 6, 7

  **References**:
  - `https://github.com/anomalyco/opencode/issues/9385` - plugin-in-the-loop memory growth case.
  - `https://github.com/anomalyco/opencode/issues/10913` - long-running session leak matrix.
  - `https://github.com/anomalyco/opencode/issues/9743` - catastrophic OOM case for safety-oriented stress testing.
  - `https://github.com/code-yeongyu/oh-my-openagent/issues/361` - real-world idle-session memory growth report.
  - `https://github.com/anomalyco/opencode/releases/tag/v1.2.20` - reminder that upstream fixes evolve and compatibility notes should be explicit.

  **Acceptance Criteria**:
  - [ ] `cargo test -p opencode-guardian --test e2e_enforce_mode -- --exact multisignal_hung_subagent_triggers_term_then_kill` -> `... ok`
  - [ ] `cargo test -p opencode-guardian --test race_conditions -- --exact guardian_restart_does_not_duplicate_action` -> `... ok`
  - [ ] `cargo test -p opencode-guardian --test soak -- --exact long_running_fixture_stays_within_memory_budget` -> `... ok`
  - [ ] CI workflow passes on `ubuntu-latest` and `macos-latest`

  **Agent-Executed QA Scenarios**:

  ```text
  Scenario: Enforce mode remediates only a validated hung fixture
    Tool: Bash
    Preconditions: Fixture config at fixtures/config/enforce.toml and owned fixture process tree available
    Steps:
      1. Run: cargo test -p opencode-guardian --test e2e_enforce_mode -- --exact multisignal_hung_subagent_triggers_term_then_kill
      2. Assert: exit code is 0
      3. Assert: output shows evidence captured before action
      4. Capture: .sisyphus/evidence/task-8-enforce-mode.txt
    Expected Result: end-to-end remediation works only for the approved fixture target and follows the staged ladder
    Evidence: .sisyphus/evidence/task-8-enforce-mode.txt

  Scenario: Soak fixture stays within budget and does not thrash
    Tool: Bash
    Preconditions: Soak fixture under tests/fixtures/soak/
    Steps:
      1. Run: cargo test -p opencode-guardian --test soak -- --exact long_running_fixture_stays_within_memory_budget
      2. Assert: exit code is 0
      3. Assert: output contains max RSS below configured threshold and zero duplicate actions
      4. Capture: .sisyphus/evidence/task-8-soak.txt
    Expected Result: guardian overhead remains bounded and action budgets prevent oscillation
    Evidence: .sisyphus/evidence/task-8-soak.txt
  ```

  **Commit**: YES
  - Message: `test(guardian): harden e2e fixtures and release workflow`
  - Files: `tests/e2e_*.rs`, `tests/soak.rs`, `.github/workflows/ci.yml`, `README.md`, `docs/`
  - Pre-commit: `cargo test --workspace`

---

## Commit Strategy

| After Task | Message | Files | Verification |
|------------|---------|-------|--------------|
| 0 | `build(guardian): bootstrap workspace and test harness` | workspace + tests + fixtures | `cargo test --workspace` |
| 2 | `feat(guardian): add monitors and safety contract` | config + monitor modules | `cargo test -p opencode-guardian --test process_inventory` |
| 4 | `feat(guardian): add policy and evidence pipeline` | policy + evidence modules | `cargo test -p opencode-guardian --test remediation_ladder` |
| 5 | `feat(guardian): add remediation and cleanup executors` | remediation + cleanup modules | `cargo test -p opencode-guardian --test process_safety` |
| 6 | `feat(guardian): integrate daemon loop and metrics` | daemon + CLI + metrics | `cargo test -p opencode-guardian --test e2e_observe_mode` |
| 7 | `feat(guardian): add optional ipc and packaging artifacts` | ipc + packaging | `cargo test -p opencode-guardian --test ipc_surface` |
| 8 | `test(guardian): harden e2e fixtures and release workflow` | e2e + soak + docs + CI | `cargo test --workspace` |

---

## Success Criteria

### Verification Commands

```bash
cargo fmt --all -- --check
# Expected: exit code 0

cargo clippy --workspace --all-targets --all-features -- -D warnings
# Expected: exit code 0

cargo test --workspace
# Expected: test result: ok

cargo test -p opencode-guardian --test remediation_ladder -- --exact single_signal_no_action
# Expected: ... ok

cargo test -p opencode-guardian --test process_safety -- --exact never_remediate_non_owned_process
# Expected: ... ok

cargo test -p opencode-guardian --test storage_boundary -- --exact only_allowlisted_artifacts_removed
# Expected: ... ok

cargo test -p opencode-guardian --test race_conditions -- --exact active_session_not_deleted_during_write
# Expected: ... ok

cargo test -p opencode-guardian --test e2e_observe_mode -- --exact observe_mode_captures_evidence_without_action
# Expected: ... ok

cargo test -p opencode-guardian --test e2e_enforce_mode -- --exact multisignal_hung_subagent_triggers_term_then_kill
# Expected: ... ok
```

### Final Checklist
- [ ] Observe-only is the default startup mode
- [ ] Same-UID and canonical-path checks are enforced before remediation
- [ ] Single-signal or ambiguous evidence never triggers destructive action
- [ ] Conservative cleanup never touches broader project state
- [ ] Evidence bundles are captured before destructive action and redact sensitive content
- [ ] Metrics stay localhost-only
- [ ] CI passes on `macOS` and `Linux`

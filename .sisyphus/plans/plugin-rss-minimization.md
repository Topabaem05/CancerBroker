# Minimize CancerBroker Plugin RSS

## TL;DR

> **Quick Summary**: Reduce CancerBroker's resident memory by turning the rust-analyzer guard into a first-class minimal runtime path, then removing unrelated long-lived allocations from the plugin-facing process before chasing micro-optimizations.
>
> **Deliverables**:
> - Reproducible RSS baseline + evidence capture for the current plugin-facing process
> - Minimal rust-analyzer-only `ra-guard` subcommand backed by deferred construction
> - Targeted rust-analyzer process sampler that avoids whole-system work where possible
> - Regression tests for guard semantics and automated RSS acceptance checks
> - Config/setup/docs updates only where the new mode or behavior requires them
>
> **Estimated Effort**: Large
> **Parallel Execution**: YES - 3 waves
> **Critical Path**: Task 1 -> Task 2 -> Task 3 -> Task 5

---

## Context

### Original Request
Research rust-analyzer memory optimization, proc-macro avoidance, and adaptive behavior, then turn the result into a repo-specific work plan. The user clarified the real goal: when this plugin operates in this mode, the plugin itself should use as little memory as possible.

### Interview Summary
**Key Discussions**:
- Priority is **plugin RSS first**, not total `plugin + rust-analyzer` memory.
- The requested output is a **work plan**, not a code change.
- The relevant scope is **CancerBroker's own rust-analyzer guard path**, not generating editor-side rust-analyzer settings.

**Research Findings**:
- `src/config.rs:176` defines a small `RustAnalyzerMemoryGuardPolicy`; the guard state itself is lightweight.
- `src/setup.rs:317` already applies adaptive RAM-bucket defaults for the guard policy written to `~/.config/cancerbroker/config.toml`.
- `src/daemon.rs:391` currently keeps the rust-analyzer guard inside a larger long-lived daemon loop that also initializes `sysinfo::System`, `StorageSnapshotCache`, cleanup engine, leak detector, and completion listener.
- `src/monitor/process.rs:163` performs broad process collection with command-line assembly and listening-port lookup.
- `src/monitor/storage.rs:193` recursively scans allowlisted roots and materializes a storage snapshot.
- Official rust-analyzer docs confirm `procMacro.enable`, `cargo.buildScripts.enable`, `cachePriming.enable`, `cargo.noDeps`, `numThreads`, `lru.capacity`, and diagnostics behavior, but **do not** provide official numeric tuning guidance matching the folklore summary.
- GitHub/RFC research confirms there is still **no built-in persistent disk cache** in rust-analyzer, proc-macro/build-script pain is still active, and RFC 3697/3698 are real but still **nightly/experimental**, not stable.

### Defaults Applied
- **Primary target process**: `cancerbroker daemon`
- **Primary platform**: macOS first
- **Preferred direction**: introduce a dedicated **`ra-guard` subcommand** backed by a rust-analyzer-only minimal runtime path
- **Idle RSS budget formula**: `min(40 MiB, floor(baseline_idle_rss_mib * 0.5))`
- **Peak RSS budget formula**: `min(64 MiB, floor(baseline_peak_rss_mib * 0.65))`
- **Bounded-growth budget**: RSS increase across 10 idle cycles `<= 5 MiB`

### Metis Review
**Identified Gaps** (addressed in this plan):
- Missing process target -> defaulted to `daemon` first because the long-lived rust-analyzer path currently lives in `src/daemon.rs:391`.
- Missing architectural direction -> defaulted to a **minimal rust-analyzer-only runtime path** before considering helper-process splitting.
- Missing numeric stop condition -> defaulted to baseline-derived RSS budgets plus bounded-growth limits.
- Missing scope guardrails -> explicitly excluded editor rust-analyzer tuning, proc-macro/build-script config generation, and generic repo-wide memory slimming.

### Oracle Recommendation
- Treat the rust-analyzer guard as a **first-class minimal runtime**.
- Avoid initializing storage snapshotting, cleanup engine, leak detection, and broad inventory work in the plugin-facing process unless profiling proves they are required.
- Only consider a helper-process split if in-process gating still cannot meet the RSS budget.

---

## Work Objectives

### Core Objective
Lower CancerBroker's plugin-facing RSS while preserving the current rust-analyzer memory-guard behavior: same-UID checks, command matching, startup grace, required consecutive samples, cooldown, and observe/enforce semantics.

### Concrete Deliverables
- A reproducible RSS measurement harness and evidence outputs for the plugin-facing process
- A minimal rust-analyzer-only runtime path for the plugin-facing process
- A narrower rust-analyzer process sampling path that avoids unrelated per-process work where possible
- Regression tests for existing guard semantics and new mode/config behavior
- Updated fixtures/docs/setup wiring only if the new mode or config requires it

### Definition of Done
- [x] `cargo build --profile release-size` succeeds
- [x] `cargo test --workspace memory_guard_ -- --nocapture` succeeds
- [x] `cargo test --workspace run_rust_analyzer_memory_guard -- --nocapture` succeeds
- [x] `cargo test --workspace` succeeds
- [x] macOS RSS evidence exists for baseline and minimized mode in `.sisyphus/evidence/`
- [x] idle RSS in minimized mode is `<= min(40 MiB, floor(baseline_idle_rss_mib * 0.5))`
- [x] peak RSS in minimized mode is `<= min(64 MiB, floor(baseline_peak_rss_mib * 0.65))`
- [x] RSS growth across 10 idle cycles is `<= 5 MiB`
- [x] guard semantics remain unchanged for startup grace, consecutive samples, cooldown, and observe/enforce behavior

### Must Have
- A clear boundary between **plugin-facing rust-analyzer guard work** and unrelated daemon subsystems
- Measurement-first validation before and after the refactor
- Deferred or conditional construction for heavy subsystems not needed in minimal mode
- Automated regression coverage for guard behavior and any new config/mode surface

### Must NOT Have (Guardrails)
- No `.vscode`, workspace, or editor rust-analyzer settings generator
- No repo-wide “memory optimization” project unrelated to the plugin-facing guard path
- No speculative stable proc-macro replacement work based on nightly-only RFCs
- No setup wizard redesign unless a new runtime mode/config switch makes it necessary
- No success criteria based only on binary size or CPU unless tied back to measured RSS
- No manual verification steps in acceptance criteria

---

## Verification Strategy

> **UNIVERSAL RULE: ZERO HUMAN INTERVENTION**
>
> All verification must be executable by the agent with commands and captured evidence. No manual editor clicking, no visual confirmation, no “user checks this locally”.

### Test Decision
- **Infrastructure exists**: YES
- **Automated tests**: Tests-after
- **Framework**: `cargo test`

### Verification Approach
- Use Rust tests for behavior preservation and CLI/bash harnesses for RSS evidence.
- Measure the actual plugin-facing process, not just unit-test heap behavior.
- Separate **idle steady-state RSS**, **peak RSS during guard activity**, and **bounded-growth over repeated cycles**.
- Preserve semantics by keeping existing memory-guard and daemon tests green before accepting RSS wins.

### Agent-Executed QA Scenarios (applies throughout the plan)

Scenario: Baseline current daemon RSS
  Tool: Bash
  Preconditions: release-size binary built; current fixture config available at `fixtures/config/completion-cleanup.toml`
  Steps:
    1. Run: `cargo build --profile release-size`
    2. Start: `target/release-size/cancerbroker --config fixtures/config/completion-cleanup.toml daemon --json --max-events 1 > .sisyphus/evidence/task-1-daemon-baseline.out 2> .sisyphus/evidence/task-1-daemon-baseline.err &`
    3. Capture spawned PID from the shell job
    4. Wait 5 seconds
    5. Run: `ps -o rss= -p <pid> > .sisyphus/evidence/task-1-daemon-baseline-rss.txt`
    6. Stop the process cleanly
  Expected Result: Baseline RSS evidence exists for the current daemon path
  Failure Indicators: process exits early; no RSS sample; command errors
  Evidence: `.sisyphus/evidence/task-1-daemon-baseline.out`, `.sisyphus/evidence/task-1-daemon-baseline.err`, `.sisyphus/evidence/task-1-daemon-baseline-rss.txt`

Scenario: Guard semantics preserved in observe mode
  Tool: Bash
  Preconditions: workspace builds
  Steps:
    1. Run: `cargo test --workspace run_rust_analyzer_memory_guard_reports_candidates_in_observe_mode -- --nocapture`
    2. Capture stdout/stderr to `.sisyphus/evidence/task-5-observe-mode-test.txt`
  Expected Result: Test passes with exit code 0
  Failure Indicators: non-zero exit code; assertion failure; changed candidate/remediation counts
  Evidence: `.sisyphus/evidence/task-5-observe-mode-test.txt`

Scenario: Guard semantics preserved in enforce mode
  Tool: Bash
  Preconditions: Unix-like environment; workspace builds
  Steps:
    1. Run: `cargo test --workspace run_rust_analyzer_memory_guard_terminates_process_in_enforce_mode -- --nocapture`
    2. Capture stdout/stderr to `.sisyphus/evidence/task-5-enforce-mode-test.txt`
  Expected Result: Test passes with exit code 0
  Failure Indicators: non-zero exit code; child process not remediated; assertion failure
  Evidence: `.sisyphus/evidence/task-5-enforce-mode-test.txt`

---

## Execution Strategy

### Parallel Execution Waves

```text
Wave 1 (Start Immediately):
- Task 1: Build RSS baseline + measurement harness
- Task 2: Create minimal runtime boundary for rust-analyzer guard

Wave 2 (After Wave 1):
- Task 3: Implement targeted rust-analyzer process sampling
- Task 4: Wire CLI/config/setup/docs for `ra-guard`

Wave 3 (After Wave 2):
- Task 5: Add regression checks and enforce RSS budgets

Critical Path: Task 1 -> Task 2 -> Task 3 -> Task 5
Parallel Speedup: ~25-35% faster than strict sequential execution
```

### Dependency Matrix

| Task | Depends On | Blocks | Can Parallelize With |
|------|------------|--------|----------------------|
| 1 | None | 3, 5 | 2 |
| 2 | None | 3, 4, 5 | 1 |
| 3 | 1, 2 | 5 | 4 |
| 4 | 2 | 5 | 3 |
| 5 | 1, 3, 4 | None | None |

### Agent Dispatch Summary

| Wave | Tasks | Recommended Agents |
|------|-------|-------------------|
| 1 | 1, 2 | `task(category="unspecified-high", load_skills=["review"], run_in_background=false)` |
| 2 | 3, 4 | `task(category="unspecified-high", load_skills=["review"], run_in_background=false)` |
| 3 | 5 | `task(category="unspecified-high", load_skills=["review"], run_in_background=false)` |

---

## TODOs

> Implementation + verification stay together. Every task includes explicit references, exclusions, and agent-executed QA.

- [x] 1. Establish RSS baseline and add a reproducible measurement harness

  **What to do**:
  - Add a reproducible measurement script at `scripts/measure_ra_guard_rss.sh`.
  - Add a reproducible RSS measurement path for the current plugin-facing process on macOS first.
  - Capture idle steady-state RSS, peak RSS during a guard cycle, and bounded growth across 10 idle cycles.
  - Save baseline evidence in `.sisyphus/evidence/` and codify the baseline-to-budget calculation used later in the plan.
  - Add a fixture or harness command dedicated to the rust-analyzer guard scenario instead of relying only on the current completion-cleanup fixture.

  **Must NOT do**:
  - Do not start by changing daemon architecture before a baseline exists.
  - Do not use manual Activity Monitor screenshots or ad-hoc local observations as evidence.
  - Do not treat CPU or binary-size deltas as substitute acceptance criteria.

  **Recommended Agent Profile**:
  - **Category**: `unspecified-high`
    - Reason: performance work needs careful measurement design and repo-specific judgment.
  - **Skills**: [`review`]
    - `review`: useful for making sure the harness measures the correct process and avoids misleading evidence.
  - **Skills Evaluated but Omitted**:
    - `git-master`: no git-history work is required inside this task.
    - `frontend-ui-ux`: no UI domain overlap.

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 1 (with Task 2)
  - **Blocks**: 3, 5
  - **Blocked By**: None

  **References**:
  - `src/daemon.rs:344` - one-shot daemon path; useful as a comparison point for minimal runtime evidence.
  - `src/daemon.rs:391` - long-lived daemon loop; this is the current primary RSS target.
  - `src/cli.rs:40` - existing `daemon` subcommand surface to mirror when introducing `ra-guard`.
  - `Cargo.toml:46` - existing `release-size` profile for consistent memory measurement builds.
  - `README.md:108` - current daemon quick-start commands; use them to keep verification aligned with documented flows.
  - `fixtures/config/completion-cleanup.toml:1` - current daemon fixture; use as the baseline control case.

  **Acceptance Criteria**:
  - [x] A reproducible harness command exists at `scripts/measure_ra_guard_rss.sh` for baseline RSS measurement on macOS.
  - [x] Baseline idle RSS evidence exists at `.sisyphus/evidence/task-1-daemon-baseline-rss.txt`.
  - [x] Baseline peak RSS evidence exists at `.sisyphus/evidence/task-1-daemon-peak-rss.txt`.
  - [x] Baseline bounded-growth evidence exists for 10 idle cycles.
  - [x] The plan's budget formulas are computable from captured evidence.

  **Agent-Executed QA Scenarios**:

  ```bash
  Scenario: Capture current daemon idle RSS
    Tool: Bash
    Preconditions: macOS host, `cargo build --profile release-size` succeeds, `scripts/measure_ra_guard_rss.sh` exists
    Steps:
      1. Run `cargo build --profile release-size`
      2. Run `scripts/measure_ra_guard_rss.sh baseline-idle .sisyphus/evidence/task-1-daemon-baseline-rss.txt`
    Expected Result: RSS sample file exists and contains a single numeric KiB value
    Failure Indicators: no PID, empty RSS file, process exits before sampling
    Evidence: `.sisyphus/evidence/task-1-daemon-baseline-rss.txt`

  Scenario: Capture current daemon peak RSS during one guard cycle
    Tool: Bash
    Preconditions: macOS host, `scripts/measure_ra_guard_rss.sh` exists
    Steps:
      1. Run `scripts/measure_ra_guard_rss.sh baseline-peak .sisyphus/evidence/task-1-daemon-peak-rss.txt`
    Expected Result: peak RSS sample file exists and contains a single numeric KiB value
    Failure Indicators: missing peak sample, malformed output, command failure
    Evidence: `.sisyphus/evidence/task-1-daemon-peak-rss.txt`

  Scenario: Capture bounded growth across idle cycles
    Tool: Bash
    Preconditions: same build as above; `scripts/measure_ra_guard_rss.sh` exists
    Steps:
      1. Run `scripts/measure_ra_guard_rss.sh growth-10 .sisyphus/evidence/task-1-daemon-growth.txt`
    Expected Result: growth evidence exists and later tasks can compare it to the `<= 5 MiB` bound
    Failure Indicators: missing samples, malformed output, uncontrolled process exit
    Evidence: `.sisyphus/evidence/task-1-daemon-growth.txt`
  ```

  **Commit**: NO
  - Message: n/a
  - Files: `scripts/measure_ra_guard_rss.sh` plus evidence path definitions
  - Pre-commit: `cargo test --workspace`

- [x] 2. Create a minimal rust-analyzer-only runtime boundary

  **What to do**:
  - Refactor the plugin-facing runtime so a dedicated `ra-guard` subcommand can run without constructing unrelated long-lived subsystems.
  - Ensure the minimal path avoids `StorageSnapshotCache`, cleanup engine, leak detector, and any other resident state not required for rust-analyzer memory guarding.
  - Prefer explicit mode-aware construction over partial "turn features off later" logic.
  - Define a dedicated `ra-guard` subcommand instead of overloading the existing `daemon` subcommand.

  **Must NOT do**:
  - Do not change guard semantics while shrinking initialization.
  - Do not keep heavy subsystems behind `Option<>` if they are still eagerly constructed.
  - Do not introduce editor-side rust-analyzer configuration work.

  **Recommended Agent Profile**:
  - **Category**: `unspecified-high`
    - Reason: this is a structural runtime refactor with behavior-preservation constraints.
  - **Skills**: [`review`]
    - `review`: useful for catching structural regressions and hidden side effects in daemon initialization.
  - **Skills Evaluated but Omitted**:
    - `git-master`: no history or commit surgery required.

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 1 (with Task 1)
  - **Blocks**: 3, 4, 5
  - **Blocked By**: None

  **References**:
  - `src/daemon.rs:302` - current rust-analyzer guard execution contract that must remain intact.
  - `src/daemon.rs:344` - current one-shot path that already constructs more than the guard needs.
  - `src/daemon.rs:391` - current long-lived loop that eagerly constructs multiple subsystems.
  - `src/cli.rs:28` - current command surface; extend it with a dedicated `ra-guard` entrypoint.
  - `src/config.rs:176` - existing rust-analyzer guard policy shape that any new mode must continue to honor.
  - `README.md:102` - current documented execution model for `setup`, `mcp`, `run-once`, and `daemon`; update only if behavior changes.

  **Acceptance Criteria**:
  - [x] A plugin-facing `ra-guard` subcommand exists.
  - [x] In that minimal path, unrelated daemon subsystems are not constructed before the guard loop starts.
  - [x] Existing `daemon` behavior remains available for the full monitoring path.
  - [x] The minimal path still produces rust-analyzer candidate/remediation counts in machine-readable output.

  **Agent-Executed QA Scenarios**:

  ```bash
  Scenario: `ra-guard` starts without full daemon side effects
    Tool: Bash
    Preconditions: `ra-guard` implemented; release-size build available
    Steps:
      1. Run `target/release-size/cancerbroker --config fixtures/config/rust-analyzer-guard-minimal.toml ra-guard --json --max-events 1 > .sisyphus/evidence/task-2-minimal-mode.out 2> .sisyphus/evidence/task-2-minimal-mode.err &`
      2. Wait 5 seconds
      3. Assert process is still alive
      4. Stop the process cleanly
      5. Inspect output for rust-analyzer memory counters only
    Expected Result: minimal runtime starts and reports guard-related fields without requiring full cleanup-loop state
    Failure Indicators: startup failure, missing JSON output, crash on idle
    Evidence: `.sisyphus/evidence/task-2-minimal-mode.out`

  Scenario: Full daemon path still works
    Tool: Bash
    Preconditions: release-size build available
    Steps:
      1. Run `target/release-size/cancerbroker --config fixtures/config/completion-cleanup.toml daemon --json --max-events 1 > .sisyphus/evidence/task-2-full-daemon.out 2> .sisyphus/evidence/task-2-full-daemon.err`
      2. Capture exit code
      3. Assert JSON contains legacy daemon counters including leak and rust-analyzer fields
    Expected Result: full daemon behavior remains intact
    Failure Indicators: missing fields, non-zero exit code, incompatible JSON shape
    Evidence: `.sisyphus/evidence/task-2-full-daemon.out`
  ```

  **Commit**: YES (groups with 3)
  - Message: `refactor(daemon): add minimal rust-analyzer runtime path`
  - Files: `src/daemon.rs`, `src/cli.rs`, `src/config.rs`
  - Pre-commit: `cargo test --workspace run_rust_analyzer_memory_guard -- --nocapture`

- [x] 3. Narrow rust-analyzer process sampling to avoid whole-system work

  **What to do**:
  - Implement a rust-analyzer-targeted process sampling path for the minimal runtime.
  - Avoid listening-port lookup, broad command-string assembly, and whole-system retained state unless a process has already passed the rust-analyzer candidate filter.
  - Keep enough identity data to preserve same-UID and command-marker safety checks.
  - Reuse selective `sysinfo` refresh APIs if they are sufficient; otherwise introduce a thinner process probe for the minimal mode only.

  **Must NOT do**:
  - Do not drop `same_uid_only` or command-marker validation.
  - Do not keep the whole-process inventory around in minimal mode if only a subset is needed.
  - Do not optimize string-level details before removing category-level allocations.

  **Recommended Agent Profile**:
  - **Category**: `unspecified-high`
    - Reason: process probing is correctness-sensitive and performance-sensitive.
  - **Skills**: [`review`]
    - `review`: useful for ensuring the narrowed sampler still respects safety boundaries.
  - **Skills Evaluated but Omitted**:
    - `git-master`: no repository-history work required.

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 2 (with Task 4)
  - **Blocks**: 5
  - **Blocked By**: 1, 2

  **References**:
  - `src/monitor/process.rs:156` - current broad collection entrypoint.
  - `src/monitor/process.rs:163` - current `collect_live_with` path that assembles command strings and loads listening ports for all processes.
  - `src/daemon.rs:411` - current loop callsite for broad live-process collection.
  - `src/memory_guard.rs:71` - startup grace gate that must remain correct.
  - `src/memory_guard.rs:100` - current guard logic and candidate creation contract.
  - `src/safety.rs:18` - `OwnershipPolicy` requirements the new sampler must still satisfy.
  - `src/safety.rs:50` - `validate_process_identity` contract that must remain enforceable.

  **Acceptance Criteria**:
  - [x] Minimal mode no longer uses the broad `collect_live_with` + listening-port path for every cycle unless profiling proves it is still required.
  - [x] Same-UID and command-marker validation still pass against the narrowed process identity surface.
  - [x] Existing memory-guard tests remain green.
  - [x] No false positive rust-analyzer candidates appear when scanning non-matching processes.

  **Agent-Executed QA Scenarios**:

  ```bash
  Scenario: Non-rust-analyzer processes remain ignored
    Tool: Bash
    Preconditions: workspace builds
    Steps:
      1. Run `cargo test --workspace memory_guard_ignores_non_matching_processes -- --nocapture > .sisyphus/evidence/task-3-ignore-non-matching.txt 2>&1`
      2. Capture exit code
      3. Assert exit code is 0
    Expected Result: narrowed sampling still ignores non-matching processes
    Failure Indicators: assertion failure or non-zero exit
    Evidence: `.sisyphus/evidence/task-3-ignore-non-matching.txt`

  Scenario: Guard still emits a candidate after the required sample count
    Tool: Bash
    Preconditions: workspace builds
    Steps:
      1. Run `cargo test --workspace memory_guard_emits_candidate_after_required_samples -- --nocapture > .sisyphus/evidence/task-3-required-samples.txt 2>&1`
      2. Capture exit code
      3. Assert exit code is 0
    Expected Result: candidate emission semantics are unchanged
    Failure Indicators: assertion failure or non-zero exit
    Evidence: `.sisyphus/evidence/task-3-required-samples.txt`
  ```

  **Commit**: YES (groups with 2)
  - Message: `perf(process): narrow rust-analyzer sampling path`
  - Files: `src/monitor/process.rs`, `src/memory_guard.rs`, `src/safety.rs`, `src/daemon.rs`
  - Pre-commit: `cargo test --workspace memory_guard_ -- --nocapture`

- [x] 4. Wire minimal mode through CLI/config/setup/docs only where necessary

  **What to do**:
  - Add the smallest config and CLI surface needed to expose the minimal rust-analyzer guard mode.
  - Add or update a dedicated fixture config for the minimal mode.
  - Update setup/docs only if the new mode or config is user-visible; keep changes narrow and explicit.
  - Preserve backward compatibility for existing configs unless a migration is unavoidable.

  **Must NOT do**:
  - Do not redesign the entire setup wizard.
  - Do not generate per-project rust-analyzer settings.
  - Do not change existing default guard thresholds unless required by the new mode.

  **Recommended Agent Profile**:
  - **Category**: `unspecified-high`
    - Reason: this task mixes CLI/config compatibility with documentation correctness.
  - **Skills**: [`review`]
    - `review`: useful for catching compatibility and behavioral drift.
  - **Skills Evaluated but Omitted**:
    - `frontend-ui-ux`: no frontend scope.

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 2 (with Task 3)
  - **Blocks**: 5
  - **Blocked By**: 2

  **References**:
  - `src/cli.rs:28` - current command enum; add `ra-guard` here.
  - `src/setup.rs:358` - current wizard defaults flow; touch only if the mode becomes setup-visible.
  - `src/setup_ui.rs:29` - current setup wizard wording; update only if the new mode is user-facing.
  - `src/config.rs:176` - current guard policy; extend carefully if a new runtime-mode field is needed.
  - `README.md:102` - current execution model docs.
  - `README.md:108` - current quick-start commands that may need a minimal-mode example.
  - `fixtures/config/observe-only.toml:1` - simplest existing config fixture pattern.
  - `fixtures/config/completion-cleanup.toml:1` - existing daemon fixture to mirror for the minimal path.

  **Acceptance Criteria**:
  - [x] A dedicated minimal-mode fixture exists.
  - [x] CLI/config parsing tests cover the new `ra-guard` surface.
  - [x] Existing config files continue to load without migration errors.
  - [x] Docs mention the minimal mode only if it is user-visible in the final design.

  **Agent-Executed QA Scenarios**:

  ```bash
  Scenario: Minimal fixture parses and starts `ra-guard`
    Tool: Bash
    Preconditions: fixture file created at `fixtures/config/rust-analyzer-guard-minimal.toml`
    Steps:
      1. Run `target/release-size/cancerbroker --config fixtures/config/rust-analyzer-guard-minimal.toml ra-guard --json --max-events 1 > .sisyphus/evidence/task-4-minimal-fixture.out 2> .sisyphus/evidence/task-4-minimal-fixture.err`
      2. Capture exit code
      3. Assert exit code is 0
    Expected Result: fixture is valid and boots `ra-guard`
    Failure Indicators: config parse error, unknown command, non-zero exit
    Evidence: `.sisyphus/evidence/task-4-minimal-fixture.out`

  Scenario: Existing observe-only config still parses
    Tool: Bash
    Preconditions: release-size build available
    Steps:
      1. Run `target/release-size/cancerbroker --config fixtures/config/observe-only.toml status --json > .sisyphus/evidence/task-4-observe-status.out 2> .sisyphus/evidence/task-4-observe-status.err`
      2. Capture exit code
      3. Assert exit code is 0 and JSON contains `mode`
    Expected Result: backward compatibility preserved for existing configs
    Failure Indicators: config parse failure, status failure, missing JSON fields
    Evidence: `.sisyphus/evidence/task-4-observe-status.out`
  ```

  **Commit**: YES
  - Message: `docs(config): expose minimal rust-analyzer guard mode`
  - Files: `src/cli.rs`, `src/setup.rs`, `src/setup_ui.rs`, `README.md`, `fixtures/config/*`
  - Pre-commit: `cargo test --workspace load_config_parses_rust_analyzer_memory_guard_overrides -- --nocapture`

- [x] 5. Enforce RSS budgets and regression-proof the new path

  **What to do**:
  - Add automated checks for idle RSS, peak RSS, and bounded growth using the baseline formulas defined earlier.
  - Keep existing rust-analyzer guard tests green and add any new coverage needed for mode/config parsing.
  - Verify that zero-process, non-matching-process, observe-mode, and enforce-mode behavior remain correct.
  - Capture final minimized-mode evidence under the same measurement method as the baseline.

  **Must NOT do**:
  - Do not accept the work based on anecdotal local improvement.
  - Do not skip full-workspace regression runs after passing targeted tests.
  - Do not allow the minimal mode to silently change remediation safety semantics.

  **Recommended Agent Profile**:
  - **Category**: `unspecified-high`
    - Reason: this is the final integration and regression gate.
  - **Skills**: [`review`]
    - `review`: useful for the final structural and safety pass before landing.
  - **Skills Evaluated but Omitted**:
    - `git-master`: only ordinary commits are needed, not history surgery.

  **Parallelization**:
  - **Can Run In Parallel**: NO
  - **Parallel Group**: Wave 3
  - **Blocks**: None
  - **Blocked By**: 1, 3, 4

  **References**:
  - `src/memory_guard.rs:210` - required-sample behavior test to preserve.
  - `src/memory_guard.rs:233` - startup grace and cooldown test to preserve.
  - `src/memory_guard.rs:267` - non-matching-process ignore test to preserve.
  - `src/daemon.rs:926` - observe-mode daemon-level guard test.
  - `src/daemon.rs:974` - enforce-mode daemon-level remediation test.
  - `src/config.rs:176` - policy defaults that must still parse and behave correctly.
  - `Cargo.toml:46` - release-size build profile used consistently for final RSS verification.

  **Acceptance Criteria**:
  - [x] `cargo test --workspace memory_guard_ -- --nocapture` passes.
  - [x] `cargo test --workspace run_rust_analyzer_memory_guard -- --nocapture` passes.
  - [x] `cargo test --workspace` passes.
  - [x] Final idle RSS evidence is `<= min(40 MiB, floor(baseline_idle_rss_mib * 0.5))`.
  - [x] Final peak RSS evidence is `<= min(64 MiB, floor(baseline_peak_rss_mib * 0.65))`.
  - [x] Final bounded-growth evidence over 10 idle cycles is `<= 5 MiB`.

  **Agent-Executed QA Scenarios**:

  ```bash
  Scenario: Final `ra-guard` path meets idle and peak RSS budgets
    Tool: Bash
    Preconditions: baseline evidence exists; `ra-guard` implemented; release-size build available; `scripts/measure_ra_guard_rss.sh` exists
    Steps:
      1. Run `scripts/measure_ra_guard_rss.sh final-idle .sisyphus/evidence/task-5-final-idle-rss.txt`
      2. Run `scripts/measure_ra_guard_rss.sh final-peak .sisyphus/evidence/task-5-final-peak-rss.txt`
      3. Compare the captured values against the baseline-derived formulas
    Expected Result: both idle and peak samples satisfy the budget formulas
    Failure Indicators: missing evidence, non-zero exit, RSS over budget
    Evidence: `.sisyphus/evidence/task-5-final-idle-rss.txt`, `.sisyphus/evidence/task-5-final-peak-rss.txt`

  Scenario: Full regression suite remains green
    Tool: Bash
    Preconditions: workspace builds
    Steps:
      1. Run `cargo test --workspace > .sisyphus/evidence/task-5-workspace-tests.txt 2>&1`
      2. Capture exit code
      3. Assert exit code is 0
    Expected Result: full workspace regression suite passes
    Failure Indicators: non-zero exit, failing tests, panics
    Evidence: `.sisyphus/evidence/task-5-workspace-tests.txt`
  ```

  **Commit**: YES
  - Message: `test(daemon): enforce rust-analyzer guard RSS budgets`
  - Files: tests, harness, fixtures, docs touched by final verification
  - Pre-commit: `cargo test --workspace`

---

## Commit Strategy

| After Task | Message | Files | Verification |
|------------|---------|-------|--------------|
| 2 + 3 | `refactor(daemon): add minimal rust-analyzer runtime path` | `src/daemon.rs`, `src/monitor/process.rs`, `src/memory_guard.rs`, `src/safety.rs`, `src/cli.rs` | `cargo test --workspace memory_guard_ -- --nocapture` |
| 4 | `docs(config): wire minimal rust-analyzer guard mode` | `src/config.rs`, `src/setup.rs`, `src/setup_ui.rs`, `README.md`, `fixtures/config/*` | `cargo test --workspace load_config_parses_rust_analyzer_memory_guard_overrides -- --nocapture` |
| 5 | `test(daemon): enforce rust-analyzer guard rss budgets` | harness/tests/evidence wiring | `cargo test --workspace` |

---

## Success Criteria

### Verification Commands
```bash
cargo build --profile release-size
cargo test --workspace memory_guard_ -- --nocapture
cargo test --workspace run_rust_analyzer_memory_guard -- --nocapture
cargo test --workspace
```

### Final Checklist
- [x] Plugin-facing rust-analyzer guard path no longer eagerly constructs unrelated daemon subsystems
- [x] A narrower rust-analyzer process sampling path exists for the minimal mode
- [x] Existing guard semantics are preserved in observe and enforce modes
- [x] macOS idle RSS meets the baseline-derived budget
- [x] macOS peak RSS meets the baseline-derived budget
- [x] RSS remains bounded across repeated idle cycles
- [x] Existing configs remain backward compatible unless the plan explicitly added and tested a new mode field
- [x] Docs describe the minimal mode only where user-visible behavior changed

# Draft: opencode-guardian

## Requirements (confirmed)
- Goal: plan a Rust-based sidecar watchdog/supervisor for opencode plus oh-my-opencode to mitigate memory leaks, runaway loops, and stale disk state.
- Working name: `opencode-guardian`.
- Preferred architecture direction: external sidecar daemon rather than in-process plugin.
- Primary problem hypotheses supplied by user: orphaned subagent/session cleanup gaps, `session.deleted` handling gaps, background-agent manager resource leaks, storage accumulation, and unbounded output/queue growth.
- Search preference: maximize parallel exploration across codebase patterns, official docs, GitHub issues/PRs, and implementation examples.

## Technical Decisions
- Candidate implementation language: Rust preferred over Go.
- Candidate MVP capabilities: process-tree monitor, storage watcher, loop detector, forced cleanup, optional IPC, configurable thresholds.
- Candidate dependency set under evaluation: `sysinfo`, `tokio`, `notify`, `clap`, `serde`, `toml`, `tracing`, `tracing-subscriber`, `color-eyre`, `nix`.
- Architecture recommendation in progress: pure external sidecar for v1, with kill/purge gated by multi-signal evidence and sustained breach windows.
- Refined dependency direction: Tokio-centered runtime, `sysinfo` for sampling, `notify` plus `notify-debouncer-full` for filesystem events, `metrics` plus Prometheus exporter for observability, `config` plus `arc-swap` for reloadable config snapshots, and `interprocess` for future cross-platform local IPC.
- Test strategy selected: TDD.
- Platform scope selected for v1: macOS plus Linux.
- Safety posture selected for v1: safety-first.
- Default first-run mode to apply in plan: observe-only / dry-run unless enforcement is explicitly enabled.
- Default ownership rule to apply in plan: same-UID targets only, with command-line and parent-lineage verification before remediation.
- Default product positioning to apply in plan: compensating control plus diagnostic evidence collector.
- Default scope handling to apply in plan: one comprehensive plan covering core sidecar MVP first, with IPC/plugin command integration and service installation as later tasks in the same plan rather than blockers for core protection.

## Research Findings
- Local workspace at `/Users/guribbong/code/cancerbroker` is currently empty and contains no relevant source files.
- Public code search confirms `session.deleted` exists in opencode core event model and store reducers.
- Public code search confirms `subagentSessions` exists in oh-my-opencode and is populated from multiple session creation paths.
- Public code search confirms `src/features/background-agent/manager.ts` uses timer cleanup structures including `setTimeout` and `completionTimers`.
- External research tasks launched for issue validation, Rust watchdog patterns, and public-repo implementation patterns.
- The public GitHub issue pages currently resolve under `code-yeongyu/oh-my-openagent`, indicating the project naming has shifted from `oh-my-opencode` to `oh-my-openagent` while many source paths still match earlier references.
- opencode web/app reducer code includes `cleanupSessionCaches()` on `session.deleted`, which deletes `message`, `part`, `session_diff`, `todo`, `permission`, `question`, and `session_status` state for the removed session.
- opencode TUI sync code currently removes the session row on `session.deleted` but does not clear associated keyed caches in the same way, matching current public leak reports.
- Current OMO event handling clears several per-session maps on `session.deleted` (`clearSessionAgent`, fallback/session-model cleanup, tmux/LSP cleanup, MCP disconnect), but global `subagentSessions` state cleanup depends on manager/task flow and is not uniformly centralized in the shared session-state module.
- Current OMO `BackgroundManager` explicitly tracks multiple long-lived structures (`tasks`, `notifications`, `pendingNotifications`, `pendingByParent`, `queuesByKey`, `processingKeys`, `completionTimers`, `idleDeferralTimers`, `notificationQueueByParent`) and performs selective cleanup on `session.error`, `session.deleted`, `cancelTask`, and retry paths.
- Public repo-structure exploration suggests a canonical Rust MVP split between `main.rs`, `config.rs`, `monitor/*`, `policy.rs`, `state.rs`, `ipc/*`, and `evidence/*` concerns rather than a monolithic daemon file.
- Direct crate example searches confirm concrete API surface for Unix socket listening (`tokio::net::UnixListener::bind`), filesystem watching (`notify::recommended_watcher`), and cross-platform process termination (`sysinfo::Process::kill_with`).
- `sysinfo` child-process traversal is not obviously exposed as a direct `children()` API in upstream examples, so process-tree reconstruction should likely be designed around parent PID indexing rather than assuming a convenience helper.
- Rust library research recommends keeping IPC portable by preferring `interprocess` or a Unix-only MVP over prematurely locking into Unix sockets if Windows support is required.
- Rust library research recommends using external service managers (`systemd`, `launchd`, Windows Service) instead of in-process daemonization patterns.
- Rust library research flags `notify` native backends as occasionally lossy/coalesced, so any reload or cleanup trigger should be idempotent and checksum/state validated.
- Rust library research flags `color-eyre` as useful for human diagnostics, but insufficient alone for machine-readable operational telemetry.
- Rust library research suggests `rustix` is a viable modern alternative to `nix`; either choice will need `cfg(unix)` boundaries.
- Final issue-validation research confirms the strongest public references are: opencode issues `#9385`, `#10913`, `#9743`, `#5734`, `#12218`, `#12351`, `#13776`, `#9157`; opencode PRs `#10914`, `#12217`, `#9146`, `#9693`, `#14650`; and OMO/oh-my-openagent issue `#1222` plus PRs `#1058`, `#1243`, `#2143`, `#2156`, `#2293`.
- Corrected reference note: `anomalyco/opencode#14650` is an open PR, not an issue.
- Maintainer-level evidence indicates opencode does not automatically clean old session storage by default.
- Public issue history shows multiple leak fixes were proposed but left open or closed unmerged, so issue/PR closure is not a reliable signal that the shipped product is fully fixed.
- Release notes do show some targeted leak fixes shipping (for example fsmonitor daemon leak mitigation), which supports a defense-in-depth approach rather than assuming upstream is wholly broken or wholly fixed.
- The safest architecture implication remains event-agnostic reconciliation: treat `session.deleted` as a hint, but use periodic sweeps, bounded retention, orphan reaping, storage GC, and output backpressure regardless of upstream event quality.

## Oracle Guidance
- V1 boundary: keep `opencode-guardian` as a pure external sidecar with no required in-process hooks.
- Enforcement model: use a remediation ladder (`warn/throttle` -> `graceful terminate` -> `forced kill` -> `bounded cleanup` -> `controlled restart`) instead of immediate hard kills.
- Destructive-action prerequisite: capture evidence first; if evidence capture fails, fall back to non-destructive mitigation and alerting.
- False-positive mitigation: require signal quorum plus sustained windows (for example 3 of 5 intervals), cooldowns, and max-actions-per-hour.
- IPC recommendation: phase 2, disabled by default in MVP, and used only for graceful hints rather than as the primary truth source.

## Dependency Notes
- Keep `tokio::signal::ctrl_c()` as the portable shutdown baseline; add Unix-specific `SIGTERM` handling behind platform gates if needed.
- Use `sysinfo::RefreshKind::nothing()` plus targeted process refreshes to avoid unnecessarily expensive full snapshots.
- Use `notify::recommended_watcher(...)` as the default watcher entry point, with an explicit fallback plan for polling/debounced watch modes.

## Source Validation Notes
- Confirmed public source: OMO issue `#361` reports severe RAM/swap growth after leaving opencode plus OMO running for hours.
- Confirmed public source: OMO issue `#1222` describes old subagent/session cleanup as a likely major memory leak, especially in Prometheus -> Oracle verification loops.
- Confirmed public source: opencode issue `#9385` remains open and explicitly mentions OMO as the plugin in use.
- Strongly supported but still under validation: additional opencode leak reports around TUI cache cleanup, ACP event stream disposal, and long-running session leaks.
- Validation complete: additional opencode leak reports around TUI cache cleanup, ACP session stream disposal, plugin disposal, and long-running session/output retention are all publicly documented.

## Open Questions
- Default assumptions now applied for plan generation unless user overrides later:
  - Upstream patches/hooks are optional, not required for v1 correctness.
  - Evidence bundles are in scope.
  - Purge boundary is conservative: only stale session/message/session_diff artifacts under canonicalized allowlisted storage paths.
  - Broader project state is never touched by default.

## Metis Guidance
- Add explicit non-goals: no Windows in v1, no remote control backend, no generic app supervisor positioning, no required IPC/plugin integration for core loop, no broad storage vacuuming.
- Add explicit safety invariants: same-user process ownership checks, allowlisted canonical paths only, active-session grace windows, cooldowns and action budgets, evidence capture before destructive action, and observe-only fallback on uncertainty.
- Add TDD coverage for negative safety cases: single-signal no-action, cooldown blocking repeated remediation, never touching non-owned processes, allowlist-only cleanup, active-session race protection, and localhost-only metrics exposure.
- Add edge-case handling for PID reuse, watcher overflow, sleep/wake clock jumps, concurrent guardian instances, partial/corrupted artifacts, and schema/path drift across upstream versions.

## Safety Posture Decision
- **Mode**: Safety-first
- **Implication**: Prefer evidence capture, shadow mode, staged remediation, and low false-positive risk over aggressive autonomous cleanup.

## Platform Decision
- **V1 platforms**: macOS + Linux
- **Implication**: Unix-first process control, signals, Unix sockets/service-manager assumptions for the MVP; Windows intentionally deferred.

## Test Strategy Decision
- **Infrastructure exists**: No local project/test setup present yet; plan should include new test infrastructure as part of project bootstrap.
- **Automated tests**: YES (TDD)
- **Agent-Executed QA**: Mandatory in addition to automated tests.

## Scope Boundaries
- INCLUDE: leak-source validation, watchdog architecture, dependency evaluation, operational policies, and rollout/testing strategy.
- EXCLUDE: direct implementation in opencode/OMO source trees for now.

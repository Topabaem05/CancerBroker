# subagent-task-complete-autocleanup

## TL;DR

> Add immediate cleanup when a subagent task completes by introducing a long-running event ingestion/orchestration path in `opencode-guardian`.
>
> Because current public `opencode` sources expose `session.idle`, `session.status`, `session.error`, `session.deleted`, and tool-part completion via `message.part.updated`, but do **not** expose a confirmed `task.completed` event, the feature should treat completion as:
> 1. event-driven when `session.status.type == "idle"` or `session.idle` is observed,
> 2. accelerated when a correlated task/tool part reaches `status == "completed"`,
> 3. reconciled by process/storage inference when events are missing, duplicated, or out of order.

---

## Current State

- `src/runtime.rs` only provides `run_once`; there is no persistent event subscriber or dispatcher.
- `src/cli.rs` exposes `status` and `run-once` only; there is no daemon command that listens for completion events.
- `src/policy.rs` decides whether an action should execute, but has no event-source or completion-specific policy abstraction.
- `src/cleanup.rs` can remove allowlisted stale artifacts, but only when candidates are already known.
- `src/remediation.rs` can terminate processes, but it is not connected to any task/session-complete event path.

## Planning Assumptions

- Target project: this Rust repo (`opencode-guardian`), not upstream `opencode` core.
- Primary completion signal: `session.status` with `idle`, with `session.idle` as a compatibility signal.
- Earliest subagent-finish hint: `message.part.updated` for relevant task/subagent tool parts when status becomes `completed`.
- `session.deleted` remains a cleanup hint, not the only correctness mechanism.
- `session.error` is a terminal fallback and must also trigger cleanup evaluation.
- Event-driven cleanup must be idempotent and backed by periodic reconciliation.
- Cleanup scope remains conservative: only validated, allowlisted session artifacts and verified owned processes.

## Non-Goals

- No requirement for upstream patches before v1 of this feature.
- No generic event bus framework beyond what is needed for completion-triggered cleanup.
- No expansion of deletion scope beyond current allowlist boundaries.
- No dependence on a hypothetical `task.completed` event unless later source validation proves it exists.

---

## Design Direction

### Completion Signal Strategy
- **Primary**: consume `session.status` transitions to `idle`.
- **Secondary**: consume `session.idle` when emitted.
- **Acceleration hint**: consume `message.part.updated` when a correlated subagent/task tool part reaches `completed`.
- **Terminal fallbacks**: consume `session.error` and `session.deleted`.
- **Fallback**: infer completion by combining process disappearance, idle session state, and storage reconciliation.

### Canonical Event Transport
- Add a new long-running `daemon` subcommand as the single runtime entrypoint for event-driven cleanup.
- Extend the existing IPC surface from read-only status into a local-only event receiver on the configured Unix socket.
- Use newline-delimited JSON as the canonical wire format so tests can inject deterministic payloads without requiring a live upstream process.
- Canonical payloads to support in v1:

```json
{"type":"session.status","event_id":"evt-1","session_id":"ses_child","status":"idle","completed_at":"2026-03-07T23:00:00Z"}
{"type":"session.idle","event_id":"evt-2","session_id":"ses_child","completed_at":"2026-03-07T23:00:01Z"}
{"type":"session.error","event_id":"evt-3","session_id":"ses_child","completed_at":"2026-03-07T23:00:02Z"}
{"type":"session.deleted","event_id":"evt-4","session_id":"ses_child","completed_at":"2026-03-07T23:00:03Z"}
{"type":"message.part.updated","event_id":"evt-5","parent_session_id":"ses_parent","tool_name":"task","task_id":"tsk_1","child_session_id":"ses_child","part_status":"completed","completed_at":"2026-03-07T23:00:04Z"}
```

### Authoritative Correlation Rules
- Session identity is the minimum required key for cleanup.
- Tool-part completion is only actionable when it carries or can resolve a `child_session_id` from persisted task metadata.
- Add a `SessionProcessIndex` that maps `session_id -> owned pid fingerprint(s)` from process inventory snapshots.
- Add a `SessionArtifactIndex` that maps `session_id -> allowlisted artifact paths` by scanning canonical session roots and filtering filenames/metadata by session id.
- If a completion event cannot resolve a session id or cannot resolve safe candidates, record evidence and defer to reconciliation instead of guessing.

### Required New Abstractions
- `CompletionEvent` model: event id, session id, subagent/task identity if known, completed timestamp, source (`status`, `idle`, `tool_part_completed`, `error`, `deleted`, `inferred`).
- `CleanupDispatcher`: receives completion events, deduplicates them, resolves cleanup candidates, and invokes existing cleanup/remediation primitives.
- `CleanupStateStore`: persists processed/pending completion events so duplicates and restarts do not re-run cleanup unsafely.
- `CandidateResolver`: maps session/task identity to concrete process ids and allowlisted artifact paths using `SessionProcessIndex` and `SessionArtifactIndex`.

### Safety Model
- Never trust event payload paths directly.
- Resolve artifacts locally, canonicalize them, and re-validate with existing allowlist logic.
- Keep active-session grace handling intact before deleting artifacts.
- If evidence capture fails, downgrade to non-destructive behavior.
- Periodic reconciliation must remain enabled even when event ingestion is healthy.

---

## Execution Plan

- [x] 0. Add completion-event domain model and config knobs
  - Add config for completion sources, dedupe TTL, cleanup retry interval, reconciliation interval, and daemon socket path.
  - Define `CompletionEvent`, `CompletionSource`, and a minimal persistent state model.
  - Acceptance:
    - `cargo test -p opencode-guardian --test completion_config -- --exact parses_completion_cleanup_settings`
    - `cargo test -p opencode-guardian --test completion_state -- --exact duplicate_event_key_is_stable`

- [x] 1. Add event ingestion surface for session completion
  - Add a `daemon` runtime command and local socket receiver as the single event-driven entrypoint.
  - Ingest `session.status` / `session.idle` / `session.error` / `session.deleted` / `message.part.updated` payloads in the canonical NDJSON shape above.
  - Normalize those event shapes into `CompletionEvent` while preserving event source and correlation metadata.
  - Acceptance:
    - `cargo test -p opencode-guardian --test completion_ingest -- --exact status_idle_event_maps_to_completion`
    - `cargo test -p opencode-guardian --test completion_ingest -- --exact session_idle_event_maps_to_completion`
    - `cargo test -p opencode-guardian --test completion_ingest -- --exact tool_part_completed_event_maps_to_completion_hint`
    - `cargo test -p opencode-guardian --test completion_ingest -- --exact session_error_event_maps_to_terminal_cleanup`
    - `cargo test -p opencode-guardian --test completion_ingest -- --exact unsupported_event_is_ignored`
    - `cargo test -p opencode-guardian --test completion_ingest -- --exact daemon_socket_accepts_ndjson_event`

- [x] 2. Implement dispatcher + idempotency state
  - Add a long-running dispatcher that queues completion events, deduplicates them, and records processed/pending state.
  - Make restart-safe behavior explicit: pending events are retried; processed events are not re-executed inside TTL.
  - Acceptance:
    - `cargo test -p opencode-guardian --test completion_dispatch -- --exact duplicate_completion_event_is_idempotent`
    - `cargo test -p opencode-guardian --test completion_dispatch -- --exact restart_retries_pending_cleanup_once`

- [x] 3. Implement candidate resolution for immediate cleanup
  - Resolve from completion event to cleanup targets:
    - owned subagent processes
    - allowlisted session/message/session_diff artifacts
  - Build `SessionProcessIndex` from process monitor snapshots and `SessionArtifactIndex` from allowlisted storage scans.
  - Require parent-child correlation from task metadata or known subagent session mapping before using tool-part completion as an immediate trigger.
  - If `child_session_id` is absent, treat tool-part completion as evidence only; do not execute cleanup.
  - Reuse `src/remediation.rs` and `src/cleanup.rs` rather than duplicating executor logic.
  - Acceptance:
    - `cargo test -p opencode-guardian --test completion_resolution -- --exact completion_event_resolves_owned_process_targets`
    - `cargo test -p opencode-guardian --test completion_resolution -- --exact completion_event_resolves_allowlisted_artifacts_only`
    - `cargo test -p opencode-guardian --test completion_resolution -- --exact unrelated_tool_part_completion_does_not_trigger_cleanup`
    - `cargo test -p opencode-guardian --test completion_resolution -- --exact unresolved_child_session_defers_to_reconciliation`

- [x] 4. Wire event-driven cleanup with reconciliation fallback
  - Add daemon/runtime loop that performs immediate cleanup on completion event and periodic reconciliation for missed events.
  - Ensure reconciliation never widens scope beyond the same safety rules.
  - Acceptance:
    - `cargo test -p opencode-guardian --test task_complete_cleanup -- --exact completion_event_triggers_immediate_cleanup`
    - `cargo test -p opencode-guardian --test task_complete_cleanup -- --exact periodic_reconciliation_cleans_missed_events`
    - `cargo test -p opencode-guardian --test task_complete_cleanup -- --exact completion_event_respects_active_session_grace`
    - `cargo test -p opencode-guardian --test task_complete_cleanup -- --exact daemon_mode_starts_dispatch_loop`

- [x] 5. Add end-to-end daemon scenarios
  - Add fixtures that simulate:
    - event arrives once and cleanup runs immediately
    - event arrives twice and cleanup runs once
    - event never arrives and reconciliation cleans later
    - event arrives before artifact is stable and grace/retry behavior wins
  - Acceptance:
    - `cargo test -p opencode-guardian --test e2e_completion_cleanup -- --exact completion_event_path_cleans_once`
    - `cargo test -p opencode-guardian --test e2e_completion_cleanup -- --exact missed_event_path_recovers_via_reconciliation`
    - `cargo test -p opencode-guardian --test e2e_completion_cleanup -- --exact unstable_artifact_is_deferred_then_cleaned`

---

## Verification Commands

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
cargo test -p opencode-guardian --test completion_ingest -- --exact status_idle_event_maps_to_completion
cargo test -p opencode-guardian --test completion_dispatch -- --exact duplicate_completion_event_is_idempotent
cargo test -p opencode-guardian --test task_complete_cleanup -- --exact completion_event_triggers_immediate_cleanup
```

## Success Criteria

- Completion-related events trigger cleanup within the configured latency budget.
- Duplicate/out-of-order completion events do not cause duplicate destructive work.
- Missed completion events are recovered by reconciliation.
- Cleanup continues to respect allowlist, same-UID, evidence, and grace-window rules.
- The daemon can explain whether cleanup came from `event-driven` or `reconciled` source in evidence output.

## Key Risks

- Exact upstream `task.completed` event does not appear to exist in confirmed public sources; plan therefore anchors on confirmed session/tool-part signals.
- Event-only designs are unsafe; reconciliation is mandatory.
- A task-complete hook without persisted dedupe state risks repeated cleanup on restart.
- `session.idle` is deprecated upstream, so it must be treated as compatibility input rather than the sole primary signal.

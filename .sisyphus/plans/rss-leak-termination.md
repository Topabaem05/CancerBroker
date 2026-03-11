# RSS Leak Termination SDD

## Goal

Add first-class live RSS leak detection to CancerBroker so the daemon can identify opencode-related processes whose resident memory keeps growing across repeated samples and, in enforce mode, terminate the leaking process and its process group using the existing remediation safety checks. Extend completion cleanup so CancerBroker also analyzes detailed open files and ports for completed-task processes and closes those resources by remediating the owning process or process group.

## Current State

- `src/monitor/process.rs` collects one live `ProcessInventory` snapshot with PID, PGID, UID, command, ports, CPU, and RSS bytes.
- `src/policy.rs` decides abstract remediation stages from `SignalWindow`s, but those windows are currently synthetic in CLI/runtime paths.
- `src/daemon.rs` and `src/autocleanup.rs` perform real cleanup only from completion/reconciliation events.
- `src/remediation.rs` already implements safe process and process-group termination.
- `src/mcp.rs` can report scans and trigger the existing runtime one-shot path, but it does not expose live leak enforcement status or detailed open-resource analysis.

## Design

### 1. Add explicit leak detection config

Extend `GuardianConfig` with a new `LeakDetectionPolicy` section dedicated to live RSS leak enforcement.

Fields:

- `enabled: bool`
- `required_consecutive_growth_samples: usize`
- `minimum_rss_bytes: u64`
- `minimum_growth_bytes_per_sample: u64`

Defaults should be conservative and safe for opt-in enforcement.

Sampling cadence will reuse the existing daemon loop interval driven by `completion.reconciliation_interval_secs`. The leak detector will evaluate one sample per daemon cycle instead of introducing a second timer.

### 2. Add a live detector module

Create `src/leak.rs` with:

- a per-process RSS history store keyed by PID + start time
- a `LeakDetector` that ingests `ProcessInventory`
- a `LeakCandidate` output that includes `ProcessIdentity`, the current RSS bytes, baseline RSS bytes, sample count, and total growth bytes

Detection rule:

- only consider processes matching the configured command markers and ownership checks
- only consider processes above `minimum_rss_bytes`
- flag a candidate only when RSS increases by at least `minimum_growth_bytes_per_sample` for `required_consecutive_growth_samples`
- drop stale history when a process disappears or restarts with a different start time

### 3. Wire daemon reconciliation to leak enforcement

Extend daemon state so repeated daemon cycles can accumulate RSS history.

Implementation path:

- build the detector alongside the existing cleanup engine in `src/daemon.rs`
- on each daemon loop iteration, collect a fresh `ProcessInventory`
- feed the inventory into `LeakDetector`
- use the existing daemon cycle cadence from `completion.reconciliation_interval_secs` as the authoritative sampling interval
- in observe mode, record/report leak candidates without termination
- in enforce mode, remediate each candidate with existing `remediate_process` and `remediate_process_group`
- reuse existing ownership policy and term timeout settings from daemon cleanup settings
- prevent duplicate group termination by de-duplicating PGIDs per cycle

### 4. Expose leak information through MCP

Extend `src/mcp.rs` with leak-oriented tool coverage using server-local detector state persisted for the life of the stdio MCP session:

- add a mutex-protected `LeakDetector` to `CancerBrokerMcp`
- add a `scan_leaks` tool that collects a fresh inventory, advances the server-local detector, and returns current candidates plus thresholds and sample counters
- keep existing `scan`, `status`, `cleanup`, and `list_evidence`

The MCP path should report live leak candidates for an active MCP session; actual automated enforcement remains daemon-driven.

### 5. Analyze open files and ports on completed-task cleanup

Add a resource analysis module for per-process open files and sockets.

Implementation path:

- create `src/monitor/resources.rs`
- collect detailed open file and socket information for a PID using the host operating system's process inspection facilities available from Rust
- represent each entry as a structured resource record with descriptor, kind, and target string
- when `src/autocleanup.rs` remediates resolved completed-task processes, collect the open-resource snapshot before sending termination signals
- attach the collected resource snapshot to cleanup results so completed-task cleanup has an auditable record of what files and ports were open
- close those resources by terminating the owning process and, when applicable, its process group through the existing remediation path instead of attempting unsafe cross-process file-descriptor manipulation
- expose a read-only MCP resource scan so the detailed open files and ports can be inspected for matching opencode processes

### 6. Test and verify end to end

Add unit tests for:

- config defaults and parsing for `LeakDetectionPolicy`
- detector behavior for growth, non-growth, minimum RSS threshold, and process restart reset
- daemon leak remediation behavior in observe vs enforce mode using synthetic inventories
- MCP leak reporting output
- resource collector parsing and classification for files, tty handles, cwd entries, and socket endpoints
- completion cleanup results that include analyzed open resources before remediation
- MCP resource reporting output

Verification requirements:

- `cargo fmt --all`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo test --workspace`
- `cargo build --workspace`
- `lsp_diagnostics` clean on all modified files
- one sandbox run proving a leaking Rust helper is detected and terminated in enforce mode
- one sandbox or synthetic verification proving completed-task cleanup records open resources and closes them by terminating the owning process

## Files Expected To Change

- `src/config.rs`
- `src/cli.rs`
- `src/lib.rs`
- `src/daemon.rs`
- `src/autocleanup.rs`
- `src/mcp.rs`
- `src/monitor/mod.rs`
- new `src/monitor/resources.rs`
- `fixtures/config/*.toml` as needed for leak settings coverage
- new `src/leak.rs`

## Non-Goals

- replacing the existing completion-event cleanup flow
- adding new external services or non-Rust components
- adding documentation outside this SDD and necessary tests

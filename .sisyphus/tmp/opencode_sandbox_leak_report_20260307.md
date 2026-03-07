# opencode Sandbox Leak Lab Report (2026-03-07)

## Goal
Run `opencode` in a sandboxed environment, intentionally create a leak-like memory condition, verify handling steps, and capture evidence.

## Sandbox Setup
- Execution wrapper: `sandbox-exec` with profile `(version 1) (allow default)`
- Isolation: dedicated per-run dirs under `/private/tmp/opencode-leak-lab3-*`
- Env isolation variables:
  - `HOME=<lab>/home`
  - `XDG_DATA_HOME=<lab>/xdg-data`
  - `XDG_CONFIG_HOME=<lab>/xdg-config`
  - `TMPDIR=<lab>/tmp`

## Reproduction Procedure
1. Start headless server: `opencode serve` on random localhost port.
2. Warmup RSS sampling (8 samples) to determine baseline.
3. Fire repeated `opencode run --attach ...` calls to create session pressure.
4. Sample RSS after each run and compute deltas.
5. Trigger handling flow:
   - graceful termination (`SIGTERM`)
   - list and delete sandbox sessions
   - verify post-cleanup session state

Automation script used:
- `.sisyphus/tmp/opencode_leak_lab.py`

## Successful Leak-Like Run
- Lab path: `/private/tmp/opencode-leak-lab3-20260307-231937`
- Summary file: `/private/tmp/opencode-leak-lab3-20260307-231937/logs/summary.json`
- RSS series: `/private/tmp/opencode-leak-lab3-20260307-231937/logs/rss.csv`

### Memory Evidence
- Baseline RSS: `91,440 KB`
- Peak RSS: `157,312 KB`
- End RSS: `123,008 KB`
- Peak delta: `+65,872 KB` (~64.3 MB)
- End delta: `+31,568 KB` (~30.8 MB)
- Leak-like threshold flag: `true`

Key observation from `rss.csv`:
- Immediate jump from baseline to `+65,872 KB` at `main-0`
- Retained elevated memory (`+27 MB` to `+31 MB`) across fanout iterations

## Handling Verification
### Process handling
- `SIGTERM` sent: `true`
- `SIGKILL` fallback required: `false`
- Result: process exited gracefully on first-stage handling.

### Session cleanup handling
- Sessions before cleanup: `11`
- Sessions after cleanup: `0`
- Deletion attempted: `11`
- Evidence files:
  - before: `/private/tmp/opencode-leak-lab3-20260307-231937/logs/session-list-before.txt`
  - after: `/private/tmp/opencode-leak-lab3-20260307-231937/logs/session-list-after.txt`

## Important Caveat
Server logs show repeated `ProviderModelNotFoundError` for `openrouter/openai/gpt-4.1` in this sandbox context:
- `/private/tmp/opencode-leak-lab3-20260307-231937/logs/server.log`

So this run demonstrates a **failure-path-induced leak-like retention pattern** (error-heavy session churn), not a normal successful inference workload.

## Additional Footprint Snapshot (post-run)
- `xdg-data/opencode`: `14 files`, `171,502 bytes`
- `home/.cache/opencode`: `1,267 files`, `3,950,068 bytes`

## Conclusion
- Reproduction: **successful** (leak-like memory growth and retention observed in sandbox run).
- Handling: **successful** (graceful process stop and complete session cleanup to zero).
- Confidence: medium (error-path scenario is valid operationally, but not equivalent to successful-model-response leak path).

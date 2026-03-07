# opencode Sandbox Memory Leak Incident Report

Date: 2026-03-07

## Objective
Run `opencode` inside a sandbox, induce leak-like memory growth with repeated session traffic, validate handling, and capture evidence.

## Environment
- Host: macOS (Darwin 25.2.0 arm64)
- Sandbox wrapper: `/usr/bin/sandbox-exec`
- Memory sampling: `/bin/ps -o rss`
- Target command: `opencode serve` + repeated `opencode run --attach`

## Experiment A (fully isolated HOME/XDG)
- Lab: `/private/tmp/opencode-leak-lab3-20260307-231014`
- Summary: `/private/tmp/opencode-leak-lab3-20260307-231014/logs/summary.json`
- Result:
  - baseline `186,064 KB`, peak delta `0 KB`, leak-like flag `false`
  - run errors/timeouts occurred
- Root cause:
  - provider/rate-limit failures in sandbox-isolated config context

## Experiment B (sandboxed process + host opencode config)
- Lab: `/private/tmp/opencode-leak-hostcfg-20260307-233148`
- Summary: `/private/tmp/opencode-leak-hostcfg-20260307-233148/logs/summary.json`
- RSS timeline: `/private/tmp/opencode-leak-hostcfg-20260307-233148/logs/rss.csv`
- Session evidence:
  - before cleanup: `/private/tmp/opencode-leak-hostcfg-20260307-233148/logs/session-list-before.txt`
  - after cleanup: `/private/tmp/opencode-leak-hostcfg-20260307-233148/logs/session-list-after.txt`

### Key Metrics (Experiment B)
- baseline RSS: `90,064 KB`
- peak RSS: `169,920 KB` (peak delta `+79,856 KB`)
- end RSS: `150,944 KB` (end delta `+60,880 KB`)
- sample count: `22`
- client outputs: `21`
- text events: `21`
- error events: `0`
- leak-like threshold hit: `true`

### Handling Validation (Experiment B)
- process stop:
  - SIGTERM attempted: yes
  - SIGKILL required: yes
- session cleanup:
  - sessions matched by lab title prefix: `13`
  - remaining after cleanup: `0`

## Interpretation
- Leak-like growth was reproducibly observed during successful request/session churn in sandboxed execution.
- Memory did not return close to baseline before shutdown (`+60 MB` retained), satisfying the configured leak-like heuristic for this lab.
- Handling path worked:
  - runaway process required escalation to forced kill
  - lab sessions were fully cleaned up

## Artifacts to Review
- Experiment B summary: `/private/tmp/opencode-leak-hostcfg-20260307-233148/logs/summary.json`
- Experiment B RSS curve: `/private/tmp/opencode-leak-hostcfg-20260307-233148/logs/rss.csv`
- Experiment B server log: `/private/tmp/opencode-leak-hostcfg-20260307-233148/logs/server.log`
- Automation script used for isolated-lab runs: `.sisyphus/tmp/opencode_leak_lab.py`

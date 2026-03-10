# opencode-session-memory-sidebar

OpenCode custom tool package for Opencode subagent/background RAM optimization.

## Install as a global tool

Recommended no-clone install:

```bash
curl -fsSL https://raw.githubusercontent.com/Topabaem05/CancerBroker/main/install/opencode-session-memory-sidebar.sh | sh
opencode --restart
```

This installs `session_memory.js` into `~/.config/opencode/tools/` so OpenCode loads it automatically at startup as a global custom tool.

## What it does

OpenCode 1.2.22 exposes a supported custom tool API. This package installs a custom tool named `session_memory` that returns:

- live/stored session counts for the current project scope
- RAM attribution for exact session processes when PID identity matches
- Opencode-owned helper process counts and RAM totals
- conservative cleanup results for stale duplicate helper processes

Ask OpenCode to use the `session_memory` tool when you want the latest Opencode RAM/process snapshot.

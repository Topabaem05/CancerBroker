# opencode-session-memory-sidebar

OpenCode custom tool package that exposes session memory data through the supported tool system.

## Install as a global tool

Recommended no-clone install:

```bash
curl -fsSL https://raw.githubusercontent.com/Topabaem05/CancerBroker/main/install/opencode-session-memory-sidebar.sh | sh
opencode --restart
```

This installs `session_memory.js` into `~/.config/opencode/tools/` so OpenCode loads it automatically at startup as a global custom tool.

## What it does

OpenCode 1.2.22 exposes a supported custom tool API, not a public plugin sidebar API. This package installs a custom tool named `session_memory` that returns:

- live session counts
- token totals for the current and other sessions
- RAM attribution coverage and totals

Ask OpenCode to use the `session_memory` tool when you want the latest snapshot.

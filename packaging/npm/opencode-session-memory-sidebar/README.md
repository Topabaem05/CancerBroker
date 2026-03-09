# opencode-session-memory-sidebar

OpenCode plugin that exposes session memory data through a supported custom tool.

## Install via local plugin file

Recommended no-clone install:

```bash
curl -fsSL https://raw.githubusercontent.com/Topabaem05/CancerBroker/main/install/opencode-session-memory-sidebar.sh | sh
opencode --restart
```

This installs `CancerBroker.plugin.js` into `~/.config/opencode/plugins/` so OpenCode loads it automatically at startup.

## What it does

OpenCode 1.2.22 does not currently expose a public plugin sidebar API. Instead of an unsupported sidebar panel, this plugin registers a custom tool named `session_memory` that returns:

- live session counts
- token totals for the current and other sessions
- RAM attribution coverage and totals

Ask OpenCode to use the `session_memory` tool when you want the latest snapshot.

## Install via config

```json
{
  "plugin": ["opencode-session-memory-sidebar"]
}
```

Use the config form only if you explicitly want npm-package registration.

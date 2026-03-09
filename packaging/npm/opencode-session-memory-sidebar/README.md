# opencode-session-memory-sidebar

OpenCode sidebar plugin that shows live session token usage and RAM availability.

## Install via local plugin file

Recommended no-clone install:

```bash
curl -fsSL https://raw.githubusercontent.com/Topabaem05/CancerBroker/main/install/opencode-session-memory-sidebar.sh | sh
opencode --restart
```

This installs `CancerBroker.plugin.js` into `~/.config/opencode/plugins/` so OpenCode loads it automatically at startup.

## Install via config

```json
{
  "plugin": ["opencode-session-memory-sidebar"]
}
```

Use the config form only if you explicitly want npm-package registration.

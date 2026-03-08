# opencode-session-memory-sidebar-installer

Clone-free installer for the OpenCode `Session Memory` sidebar plugin.

This package does not copy files into the plugin directory. Instead, it edits `opencode.json` and adds the npm plugin package name so OpenCode installs and caches the plugin automatically on startup.

## Install (no git clone)

```bash
bunx opencode-session-memory-sidebar-installer
```

Install a scoped package name instead:

```bash
bunx opencode-session-memory-sidebar-installer --package @your-scope/opencode-session-memory-sidebar
```

Project-local config:

```bash
bunx opencode-session-memory-sidebar-installer --project
```

## Uninstall

```bash
bunx opencode-session-memory-sidebar-installer uninstall
```

## Local repository workflow

When working from this repository, you can use the root helper instead of changing into the installer directory:

```bash
./session-memory-plugin add
./session-memory-plugin add --project
./session-memory-plugin remove
```

If you want a bare command in the current shell, activate the repo-local command path from the repository root:

```bash
. ./scripts/dev-env.sh
session-memory-plugin add
session-memory-plugin remove
```

- `remove` only unregisters the plugin from `opencode.json`; it does not delete plugin files.
- Pass `--config`, `--package`, and `--project` through exactly as you would with the installer.
- Add `--restart` if you want the helper to run `opencode --restart` after the config update.

## Notes

- Edits `~/.config/opencode/opencode.json` by default
- Adds `opencode-session-memory-sidebar` to the `plugin` array idempotently
- Supports `--package` to target a scoped npm package name
- Supports JSONC comments/trailing commas
- Creates a timestamped backup before write
- Restart OpenCode after install/uninstall: `opencode --restart`

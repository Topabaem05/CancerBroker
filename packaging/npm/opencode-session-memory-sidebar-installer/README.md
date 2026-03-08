# opencode-session-memory-sidebar-installer

Clone-free installer for the OpenCode `Session Memory` sidebar plugin.

This package does not copy files into the plugin directory. Instead, it edits `opencode.json` and adds the npm plugin package name so OpenCode installs and caches the plugin automatically on startup.

## Install (no git clone)

Current private-repo flow for authenticated collaborators:

```bash
gh api "repos/Topabaem05/CancerBroker/contents/install/opencode-session-memory-sidebar.sh?ref=main" --jq .content \
  | tr -d '\n' \
  | node -e 'let data=""; process.stdin.setEncoding("utf8"); process.stdin.on("data", (chunk) => data += chunk); process.stdin.on("end", () => process.stdout.write(Buffer.from(data, "base64")));' \
  | sh
```

- Requirements:
  - `gh` authenticated with access to this repository
  - `node` or `bun` installed locally

Project-local config:

```bash
gh api "repos/Topabaem05/CancerBroker/contents/install/opencode-session-memory-sidebar.sh?ref=main" --jq .content \
  | tr -d '\n' \
  | node -e 'let data=""; process.stdin.setEncoding("utf8"); process.stdin.on("data", (chunk) => data += chunk); process.stdin.on("end", () => process.stdout.write(Buffer.from(data, "base64")));' \
  | sh -s -- --project
```

## Uninstall

```bash
gh api "repos/Topabaem05/CancerBroker/contents/install/opencode-session-memory-sidebar.sh?ref=main" --jq .content \
  | tr -d '\n' \
  | node -e 'let data=""; process.stdin.setEncoding("utf8"); process.stdin.on("data", (chunk) => data += chunk); process.stdin.on("end", () => process.stdout.write(Buffer.from(data, "base64")));' \
  | sh -s -- uninstall
```

## Future public bootstrap path

```bash
curl -fsSL https://raw.githubusercontent.com/Topabaem05/CancerBroker/main/install/opencode-session-memory-sidebar.sh | sh
```

Use the curl form only after the repository or release asset is publicly reachable. In the current private-repo setup, the bootstrap script falls back to `gh api` for authenticated access.

## Future npm path

After the installer package is published to npm, these package-exec commands will be supported too:

```bash
bunx opencode-session-memory-sidebar-installer
```

```bash
npx --yes opencode-session-memory-sidebar-installer
```

Install a scoped package name instead:

```bash
bunx opencode-session-memory-sidebar-installer --package @your-scope/opencode-session-memory-sidebar
```

```bash
npx --yes opencode-session-memory-sidebar-installer --package @your-scope/opencode-session-memory-sidebar
```

Project-local config via npm package:

```bash
bunx opencode-session-memory-sidebar-installer --project
```

```bash
npx --yes opencode-session-memory-sidebar-installer --project
```

Uninstall via npm package:

```bash
bunx opencode-session-memory-sidebar-installer uninstall
```

```bash
npx --yes opencode-session-memory-sidebar-installer uninstall
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
- Homebrew is not active yet for this private repo; it needs a public tap or release-distribution strategy first

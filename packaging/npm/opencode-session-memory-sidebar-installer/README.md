# opencode-session-memory-sidebar-installer

Clone-free installer for the OpenCode `Session Memory` sidebar plugin.

This package installs a bundled local plugin file into OpenCode's plugin directory so the sidebar works even before the npm plugin package is published.

## Install (no git clone)

Current public bootstrap:

```bash
curl -fsSL https://raw.githubusercontent.com/Topabaem05/CancerBroker/main/install/opencode-session-memory-sidebar.sh | sh
```

The bootstrap script fetches the latest published installer asset from GitHub Releases.

Installed plugin locations:

- Global: `~/.config/opencode/plugins/CancerBroker.plugin.js`
- Project: `.opencode/plugins/CancerBroker.plugin.js`

- Requirements:
  - `node` or `bun` installed locally

Reviewable two-step variant:

```bash
curl -fsSL -o /tmp/opencode-session-memory-sidebar.sh https://raw.githubusercontent.com/Topabaem05/CancerBroker/main/install/opencode-session-memory-sidebar.sh
sh /tmp/opencode-session-memory-sidebar.sh
```

Project-local config:

```bash
curl -fsSL https://raw.githubusercontent.com/Topabaem05/CancerBroker/main/install/opencode-session-memory-sidebar.sh | sh -s -- --project
```

## Uninstall

```bash
curl -fsSL https://raw.githubusercontent.com/Topabaem05/CancerBroker/main/install/opencode-session-memory-sidebar.sh | sh -s -- uninstall
```

## Authenticated fallback

```bash
gh api "repos/Topabaem05/CancerBroker/contents/install/opencode-session-memory-sidebar.sh?ref=main" --jq .content \
  | tr -d '\n' \
  | node -e 'let data=""; process.stdin.setEncoding("utf8"); process.stdin.on("data", (chunk) => data += chunk); process.stdin.on("end", () => process.stdout.write(Buffer.from(data, "base64")));' \
  | sh
```

The bootstrap script itself still falls back to `gh api` if direct raw downloads fail.

## Homebrew

Recommended direct install:

```bash
brew install topabaem05/cancerbroker/opencode-session-memory-sidebar-installer
```

If Homebrew needs an explicit tap URL because this repository is named `CancerBroker` instead of `homebrew-cancerbroker`:

```bash
brew tap topabaem05/cancerbroker https://github.com/Topabaem05/CancerBroker
brew install topabaem05/cancerbroker/opencode-session-memory-sidebar-installer
```

Uninstall with Homebrew:

```bash
brew uninstall opencode-session-memory-sidebar-installer
```

Current versioned release asset URL used by the formula:

```text
https://github.com/Topabaem05/CancerBroker/releases/download/CancerBroker-v0.1.2/CancerBroker.cjs
```

## Release automation

Prepare the next installer release from the repository root:

```bash
node ./scripts/prepare-installer-release.mjs 0.1.1
```

It updates `package.json`, rebuilds the standalone installer, refreshes the Homebrew formula `sha256`, and rewrites versioned release-asset URLs in docs before you commit and tag the release.

## Future npm path

The default installer path uses a local plugin file. If you explicitly want npm-package registration instead, pass `--package`.

```bash
curl -fsSL https://raw.githubusercontent.com/Topabaem05/CancerBroker/main/install/opencode-session-memory-sidebar.sh | sh -s -- --package opencode-session-memory-sidebar
```

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

- `remove` deletes the installed local plugin file and also removes the default npm plugin entry if it was left behind.
- Pass `--config`, `--package`, and `--project` through exactly as you would with the installer.
- Add `--restart` if you want the helper to run `opencode --restart` after the config update.

## Notes

- Installs `CancerBroker.plugin.js` into OpenCode's plugin directory by default
- Cleans up stale default npm plugin entries from `opencode.json`
- Supports `--package` to target a scoped npm package name
- Supports JSONC comments/trailing commas
- Creates a timestamped backup before write
- Restart OpenCode after install/uninstall: `opencode --restart`
- Homebrew formula path: `brew install topabaem05/cancerbroker/opencode-session-memory-sidebar-installer`
- Release asset workflow: `.github/workflows/release-installer-asset.yml`
- Release prep command: `node ./scripts/prepare-installer-release.mjs <version>`

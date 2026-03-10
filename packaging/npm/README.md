# OpenCode npm Distribution Plan

## Packages

- `opencode-session-memory-sidebar`
  - Actual OpenCode tool package.
  - Bundled into a local tool asset for clone-free installs.
  - Exposes the supported custom tool `session_memory`.
  - Focused on Opencode subagent/background RAM visibility and cleanup.

- `opencode-session-memory-sidebar-installer`
  - Small CLI package for clone-free installation.
  - Current public bootstrap command:

    ```bash
    curl -fsSL https://raw.githubusercontent.com/Topabaem05/CancerBroker/main/install/opencode-session-memory-sidebar.sh | sh
    ```

    The bootstrap script downloads the latest installer asset from GitHub Releases and installs `session_memory.js` into the OpenCode tools directory.

  - Authenticated fallback if raw fetches fail:

    ```bash
    gh api "repos/Topabaem05/CancerBroker/contents/install/opencode-session-memory-sidebar.sh?ref=main" --jq .content \
      | tr -d '\n' \
      | node -e 'let data=""; process.stdin.setEncoding("utf8"); process.stdin.on("data", (chunk) => data += chunk); process.stdin.on("end", () => process.stdout.write(Buffer.from(data, "base64")));' \
      | sh
    ```

  - Installs a local tool file by default and only touches `opencode.json` to clean stale legacy plugin entries.

## Install Channels

1. Current public curl path: bootstrap script downloads the installer without cloning the repo and installs `session_memory.js` locally.
2. Authenticated fallback path: `gh api` can bootstrap the same script if raw fetches fail.
3. Homebrew path: public tap formula from this repository.

```bash
brew install topabaem05/cancerbroker/opencode-session-memory-sidebar-installer
```

If Homebrew requires an explicit tap URL for this repository name:

```bash
brew tap topabaem05/cancerbroker https://github.com/Topabaem05/CancerBroker
brew install topabaem05/cancerbroker/opencode-session-memory-sidebar-installer
```

Current versioned release asset URL:

```text
https://github.com/Topabaem05/CancerBroker/releases/download/CancerBroker-v0.1.6/CancerBroker.cjs
```

## Publish Order

1. Publish release assets for `session_memory.js` and `CancerBroker.cjs`
2. User runs installer with public curl bootstrap, authenticated fallback, Homebrew, or future package exec
3. Installer writes `session_memory.js` into `~/.config/opencode/tools/` or `.opencode/tools/`
4. User runs `opencode --restart`
5. OpenCode loads the local tool automatically on startup

## Version Policy

- The two packages use independent semver.
- Bump `opencode-session-memory-sidebar` when runtime/tool behavior changes.
- Bump `opencode-session-memory-sidebar-installer` when install UX, config editing, or CLI behavior changes.
- If both packages change in one release, build the tool asset first, then the installer asset.

## Release Automation

Prepare the installer release files in one step:

```bash
node ./scripts/prepare-installer-release.mjs 0.1.1
```

That command updates the installer package version, rebuilds the standalone asset, refreshes the Homebrew formula `sha256`, and rewrites the versioned release-asset URLs in docs.

## GitHub Actions Publish Flow

- Workflow file: `.github/workflows/npm-publish.yml`
- Trigger: manual `workflow_dispatch`
- Safety gate: input `confirm=publish`
- Required secret: `NPM_TOKEN`
- Current default distribution does not require npm publication.

### Release Checklist

1. Run `node ./scripts/prepare-installer-release.mjs <version>`
2. Review changes and push `main`
3. Run the `npm-publish` workflow with `confirm=publish` only if you intentionally want npm-distributed packages as a secondary path
4. Push tag `CancerBroker-v<version>` or run `.github/workflows/release-installer-asset.yml`
5. Test clone-free public install with:

   ```bash
   curl -fsSL https://raw.githubusercontent.com/Topabaem05/CancerBroker/main/install/opencode-session-memory-sidebar.sh | sh
   opencode --restart
   ```

## Safety Rules

- Installer creates a timestamped backup before writing.
- Installer preserves JSONC-compatible files.
- Installer is idempotent for both install and uninstall.
- Installer supports global config by default and project config with `--project`.
- Homebrew formula is available from the public repository tap.
- Release assets are published by `.github/workflows/release-installer-asset.yml`.
- Release prep is automated by `scripts/prepare-installer-release.mjs`.
- Default installation no longer depends on npm publication of the package.

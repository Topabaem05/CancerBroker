# OpenCode npm Distribution Plan

## Packages

- `opencode-session-memory-sidebar`
  - Actual OpenCode plugin package.
  - Published to npm.
  - OpenCode installs and caches it from the `plugin` array in `opencode.json`.

- `opencode-session-memory-sidebar-installer`
  - Small CLI package for clone-free installation.
  - Current public bootstrap command:

    ```bash
    curl -fsSL https://raw.githubusercontent.com/Topabaem05/CancerBroker/main/install/opencode-session-memory-sidebar.sh | sh
    ```

    The bootstrap script downloads the latest installer asset from GitHub Releases.

  - Authenticated fallback if raw fetches fail:

    ```bash
    gh api "repos/Topabaem05/CancerBroker/contents/install/opencode-session-memory-sidebar.sh?ref=main" --jq .content \
      | tr -d '\n' \
      | node -e 'let data=""; process.stdin.setEncoding("utf8"); process.stdin.on("data", (chunk) => data += chunk); process.stdin.on("end", () => process.stdout.write(Buffer.from(data, "base64")));' \
      | sh
    ```

  - Planned package-exec command after npm publication:

    ```bash
    bunx opencode-session-memory-sidebar-installer
    ```

    ```bash
    npx --yes opencode-session-memory-sidebar-installer
    ```

  - Edits `opencode.json` safely and idempotently.

## Install Channels

1. Current public curl path: bootstrap script downloads the installer without cloning the repo.
2. Authenticated fallback path: `gh api` can bootstrap the same script if raw fetches fail.
3. Future npm path: `bunx` / `npx` once both packages are published.
4. Homebrew path: public tap formula from this repository.

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
https://github.com/Topabaem05/CancerBroker/releases/download/CancerBroker-v0.1.1/CancerBroker.cjs
```

## Publish Order

1. Publish `opencode-session-memory-sidebar`
2. Publish `opencode-session-memory-sidebar-installer`
3. User runs installer with public curl bootstrap, authenticated fallback, or future package exec
4. Installer appends `opencode-session-memory-sidebar` to `plugin` in `opencode.json`
5. User runs `opencode --restart`
6. OpenCode downloads and loads the plugin automatically

If you publish under a scope, the installer can target it with:

```bash
bunx opencode-session-memory-sidebar-installer --package @your-scope/opencode-session-memory-sidebar
```

The bootstrap simply forwards installer flags, so the same scoped install works there too:

```bash
curl -fsSL https://raw.githubusercontent.com/Topabaem05/CancerBroker/main/install/opencode-session-memory-sidebar.sh | sh -s -- --package @your-scope/opencode-session-memory-sidebar
```

## Version Policy

- The two packages use independent semver.
- Bump `opencode-session-memory-sidebar` when runtime/plugin behavior changes.
- Bump `opencode-session-memory-sidebar-installer` when install UX, config editing, or CLI behavior changes.
- If both packages change in one release, publish the plugin package first, then the installer package.
- Keep the installer's default package target aligned with the actual published plugin package name.

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
- Order enforced by workflow:
  1. validate plugin + installer packages
  2. publish `opencode-session-memory-sidebar`
  3. publish `opencode-session-memory-sidebar-installer`

### Release Checklist

1. Run `node ./scripts/prepare-installer-release.mjs <version>`
2. Review changes and push `main`
3. Run the `npm-publish` workflow with `confirm=publish` if the installer package itself should be published to npm
4. Push tag `CancerBroker-v<version>` or run `.github/workflows/release-installer-asset.yml`
5. Test clone-free public install with:

   ```bash
   curl -fsSL https://raw.githubusercontent.com/Topabaem05/CancerBroker/main/install/opencode-session-memory-sidebar.sh | sh
   opencode --restart
   ```

6. Test npm package install after publish with:

   ```bash
   bunx opencode-session-memory-sidebar-installer
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

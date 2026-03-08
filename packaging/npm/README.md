# OpenCode npm Distribution Plan

## Packages

- `opencode-session-memory-sidebar`
  - Actual OpenCode plugin package.
  - Published to npm.
  - OpenCode installs and caches it from the `plugin` array in `opencode.json`.

- `opencode-session-memory-sidebar-installer`
  - Small CLI package for clone-free installation.
  - Immediate end-user command:

    ```bash
    curl -fsSL https://raw.githubusercontent.com/Topabaem05/CancerBroker/main/install/opencode-session-memory-sidebar.sh | sh
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

1. Current clone-free path: GitHub-hosted curl bootstrap script downloads a bundled standalone installer and updates `opencode.json` without cloning the repo.
2. Future npm path: `bunx` / `npx` once both packages are published.
3. Future Homebrew path: requires a tap/formula plus release or source URLs, so it is not implemented yet in this repo.

## Publish Order

1. Publish `opencode-session-memory-sidebar`
2. Publish `opencode-session-memory-sidebar-installer`
3. User runs installer with curl bootstrap or package exec
4. Installer appends `opencode-session-memory-sidebar` to `plugin` in `opencode.json`
5. User runs `opencode --restart`
6. OpenCode downloads and loads the plugin automatically

If you publish under a scope, the installer can target it with:

```bash
bunx opencode-session-memory-sidebar-installer --package @your-scope/opencode-session-memory-sidebar
```

The curl bootstrap simply forwards installer flags, so the same scoped install works there too:

```bash
curl -fsSL https://raw.githubusercontent.com/Topabaem05/CancerBroker/main/install/opencode-session-memory-sidebar.sh | sh -s -- --package @your-scope/opencode-session-memory-sidebar
```

## Version Policy

- The two packages use independent semver.
- Bump `opencode-session-memory-sidebar` when runtime/plugin behavior changes.
- Bump `opencode-session-memory-sidebar-installer` when install UX, config editing, or CLI behavior changes.
- If both packages change in one release, publish the plugin package first, then the installer package.
- Keep the installer's default package target aligned with the actual published plugin package name.

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

1. Update the version field in each package that changed
2. Verify local checks pass
3. Run the `npm-publish` workflow with `confirm=publish`
4. Test clone-free install with:

   ```bash
   curl -fsSL https://raw.githubusercontent.com/Topabaem05/CancerBroker/main/install/opencode-session-memory-sidebar.sh | sh
   opencode --restart
   ```

5. Test npm package install after publish with:

   ```bash
   bunx opencode-session-memory-sidebar-installer
   opencode --restart
   ```

## Safety Rules

- Installer creates a timestamped backup before writing.
- Installer preserves JSONC-compatible files.
- Installer is idempotent for both install and uninstall.
- Installer supports global config by default and project config with `--project`.
- Homebrew is intentionally deferred until a tap/formula strategy is chosen.

#!/usr/bin/env node

import {
  DEFAULT_PLUGIN_PACKAGE_NAME,
  resolveTargetConfigPath,
  uninstallPluginFromConfig,
} from "./config-file.mjs";
import { uninstallLocalPlugin } from "./plugin-file.mjs";
import { uninstallLocalTool } from "./tool-file.mjs";

export default async function runUninstall(argv = process.argv.slice(2)) {
  const flags = parseFlags(argv);
  const configPath = resolveTargetConfigPath(flags);
  if (flags.packageName) {
    const pluginName = flags.packageName || DEFAULT_PLUGIN_PACKAGE_NAME;
    const result = uninstallPluginFromConfig(configPath, pluginName);

    if (result.changed) {
      console.log(`[opencode-session-memory-sidebar] Removed plugin from ${result.configPath}`);
      if (result.backupPath) {
        console.log(`[opencode-session-memory-sidebar] Backup: ${result.backupPath}`);
      }
    } else {
      console.log(`[opencode-session-memory-sidebar] Plugin not present in ${result.configPath}`);
    }
    console.log("[opencode-session-memory-sidebar] Restart OpenCode: opencode --restart");
    return;
  }

  const toolResult = uninstallLocalTool(flags);
  if (toolResult.changed) {
    console.log(`[opencode-session-memory-sidebar] Removed local tool at ${toolResult.toolFilePath}`);
    if (toolResult.backupPath) {
      console.log(`[opencode-session-memory-sidebar] Backup: ${toolResult.backupPath}`);
    }
  } else {
    console.log(`[opencode-session-memory-sidebar] Local tool not present at ${toolResult.toolFilePath}`);
  }

  const legacyPluginCleanup = uninstallLocalPlugin(flags);
  if (legacyPluginCleanup.changed) {
    console.log(`[opencode-session-memory-sidebar] Removed legacy plugin file at ${legacyPluginCleanup.pluginFilePath}`);
    if (legacyPluginCleanup.backupPath) {
      console.log(`[opencode-session-memory-sidebar] Backup: ${legacyPluginCleanup.backupPath}`);
    }
  }

  const cleanupResult = uninstallPluginFromConfig(configPath, DEFAULT_PLUGIN_PACKAGE_NAME);
  if (cleanupResult.changed) {
    console.log(`[opencode-session-memory-sidebar] Removed npm plugin entry from ${cleanupResult.configPath}`);
    if (cleanupResult.backupPath) {
      console.log(`[opencode-session-memory-sidebar] Backup: ${cleanupResult.backupPath}`);
    }
  }

  console.log("[opencode-session-memory-sidebar] Restart OpenCode: opencode --restart");
}

const invokedAsMain = import.meta.url === new URL(process.argv[1], "file:").href;
if (invokedAsMain) {
  await runUninstall(process.argv.slice(2));
}

function parseFlags(argv) {
  const flags = {
    project: false,
    configPath: undefined,
    packageName: undefined,
  };

  for (let index = 0; index < argv.length; index += 1) {
    const value = argv[index];
    if (value === "--project") {
      flags.project = true;
      continue;
    }
    if (value === "--config" && typeof argv[index + 1] === "string") {
      flags.configPath = argv[index + 1];
      index += 1;
      continue;
    }
    if (value === "--package" && typeof argv[index + 1] === "string") {
      flags.packageName = argv[index + 1];
      index += 1;
    }
  }

  return flags;
}

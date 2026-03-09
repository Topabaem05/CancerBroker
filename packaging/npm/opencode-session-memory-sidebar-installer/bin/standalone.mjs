#!/usr/bin/env node

import {
  DEFAULT_PLUGIN_PACKAGE_NAME,
  installPluginIntoConfig,
  resolveTargetConfigPath,
  uninstallPluginFromConfig,
} from "./config-file.mjs";
import { uninstallLocalPlugin } from "./plugin-file.mjs";
import { installLocalTool, uninstallLocalTool } from "./tool-file.mjs";

const args = process.argv.slice(2);
main(args).catch((error) => {
  throw error;
});

async function main(argv) {
  const command = argv[0] === "uninstall" ? "uninstall" : "install";
  const parsedArgs = command === "uninstall" ? argv.slice(1) : argv;
  const flags = parseFlags(parsedArgs);
  const configPath = resolveTargetConfigPath(flags);

  if (command === "uninstall") {
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
    return;
  }

  if (flags.packageName) {
    const pluginName = flags.packageName || DEFAULT_PLUGIN_PACKAGE_NAME;
    const result = installPluginIntoConfig(configPath, pluginName);

    if (result.changed) {
      console.log(`[opencode-session-memory-sidebar] Added plugin to ${result.configPath}`);
      if (result.backupPath) {
        console.log(`[opencode-session-memory-sidebar] Backup: ${result.backupPath}`);
      }
    } else {
      console.log(`[opencode-session-memory-sidebar] Plugin already present in ${result.configPath}`);
    }
    console.log(`[opencode-session-memory-sidebar] Plugin package: ${result.pluginName}`);
    console.log("[opencode-session-memory-sidebar] Restart OpenCode: opencode --restart");
    return;
  }

  const toolResult = await installLocalTool(flags);
  if (toolResult.changed) {
    console.log(`[opencode-session-memory-sidebar] Installed local tool at ${toolResult.toolFilePath}`);
    if (toolResult.backupPath) {
      console.log(`[opencode-session-memory-sidebar] Backup: ${toolResult.backupPath}`);
    }
  } else {
    console.log(`[opencode-session-memory-sidebar] Local tool already up to date at ${toolResult.toolFilePath}`);
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

  console.log(`[opencode-session-memory-sidebar] Tool asset: ${toolResult.toolUrl}`);
  console.log("[opencode-session-memory-sidebar] Restart OpenCode: opencode --restart");
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

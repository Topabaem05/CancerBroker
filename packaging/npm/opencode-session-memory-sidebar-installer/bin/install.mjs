#!/usr/bin/env node

import {
  DEFAULT_PLUGIN_PACKAGE_NAME,
  installPluginIntoConfig,
  resolveTargetConfigPath,
  uninstallPluginFromConfig,
} from "./config-file.mjs";
import { uninstallLocalPlugin } from "./plugin-file.mjs";
import { installLocalTool } from "./tool-file.mjs";

const args = process.argv.slice(2);
const command = args[0] === "uninstall" ? "uninstall" : "install";

if (command === "uninstall") {
  const { default: runUninstall } = await import("./uninstall.mjs");
  await runUninstall(args.slice(1));
  process.exit(0);
}

const flags = parseFlags(args);
const configPath = resolveTargetConfigPath(flags);
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
  process.exit(0);
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

const pluginCleanup = uninstallLocalPlugin(flags);
if (pluginCleanup.changed) {
  console.log(`[opencode-session-memory-sidebar] Removed legacy plugin file at ${pluginCleanup.pluginFilePath}`);
  if (pluginCleanup.backupPath) {
    console.log(`[opencode-session-memory-sidebar] Backup: ${pluginCleanup.backupPath}`);
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

#!/usr/bin/env node

import {
  DEFAULT_PLUGIN_PACKAGE_NAME,
  installPluginIntoConfig,
  resolveTargetConfigPath,
  uninstallPluginFromConfig,
} from "./config-file.mjs";

const args = process.argv.slice(2);
const command = args[0] === "uninstall" ? "uninstall" : "install";
const parsedArgs = command === "uninstall" ? args.slice(1) : args;
const flags = parseFlags(parsedArgs);
const configPath = resolveTargetConfigPath(flags);
const pluginName = flags.packageName || DEFAULT_PLUGIN_PACKAGE_NAME;

if (command === "uninstall") {
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
  process.exit(0);
}

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

import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { homedir } from "node:os";

export const DEFAULT_PLUGIN_ASSET_NAME = "CancerBroker.plugin.js";
export const DEFAULT_PLUGIN_URL =
  process.env.OPENCODE_SESSION_MEMORY_PLUGIN_URL ||
  `https://github.com/Topabaem05/CancerBroker/releases/latest/download/${DEFAULT_PLUGIN_ASSET_NAME}`;

export function resolvePluginDirectory(options = {}) {
  if (options.project) {
    return join(process.cwd(), ".opencode", "plugins");
  }

  const configDir = process.env.OPENCODE_CONFIG_DIR || join(homedir(), ".config", "opencode");
  return join(configDir, "plugins");
}

export function resolvePluginFilePath(options = {}) {
  return join(resolvePluginDirectory(options), DEFAULT_PLUGIN_ASSET_NAME);
}

export async function installLocalPlugin(options = {}) {
  const pluginUrl = options.pluginUrl || DEFAULT_PLUGIN_URL;
  const pluginFilePath = resolvePluginFilePath(options);
  const nextText = await downloadPluginAsset(pluginUrl);

  mkdirSync(resolvePluginDirectory(options), { recursive: true });

  const currentText = existsSync(pluginFilePath) ? readFileSync(pluginFilePath, "utf8") : null;
  if (currentText === nextText) {
    return {
      changed: false,
      pluginFilePath,
      pluginUrl,
      backupPath: null,
    };
  }

  const backupPath = currentText === null ? null : backupExistingFile(pluginFilePath, currentText);
  writeFileSync(pluginFilePath, ensureTrailingEol(nextText), "utf8");

  return {
    changed: true,
    pluginFilePath,
    pluginUrl,
    backupPath,
  };
}

export function uninstallLocalPlugin(options = {}) {
  const pluginFilePath = resolvePluginFilePath(options);
  if (!existsSync(pluginFilePath)) {
    return {
      changed: false,
      pluginFilePath,
      backupPath: null,
    };
  }

  const originalText = readFileSync(pluginFilePath, "utf8");
  const backupPath = backupExistingFile(pluginFilePath, originalText);

  return {
    changed: true,
    pluginFilePath,
    backupPath,
  };
}

async function downloadPluginAsset(pluginUrl) {
  if (pluginUrl.startsWith("file://")) {
    return readFileSync(new URL(pluginUrl), "utf8");
  }

  if (pluginUrl.startsWith("/") && existsSync(pluginUrl)) {
    return readFileSync(pluginUrl, "utf8");
  }

  const response = await fetch(pluginUrl);
  if (!response.ok) {
    throw new Error(`Unable to download plugin asset from ${pluginUrl}: HTTP ${response.status}`);
  }

  return await response.text();
}

function backupExistingFile(filePath, originalText) {
  const backupPath = `${filePath}.bak.${Date.now()}`;
  writeFileSync(backupPath, originalText, "utf8");
  return backupPath;
}

function ensureTrailingEol(text) {
  return text.endsWith("\n") ? text : `${text}\n`;
}

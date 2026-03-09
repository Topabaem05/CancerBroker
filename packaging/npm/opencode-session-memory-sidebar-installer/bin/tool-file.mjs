import { existsSync, mkdirSync, readFileSync, writeFileSync, rmSync } from "node:fs";
import { dirname, join } from "node:path";
import { homedir } from "node:os";

export const DEFAULT_TOOL_ASSET_NAME = "session_memory.js";
export const DEFAULT_TOOL_URL =
  process.env.OPENCODE_SESSION_MEMORY_TOOL_URL ||
  `https://github.com/Topabaem05/CancerBroker/releases/latest/download/${DEFAULT_TOOL_ASSET_NAME}`;

export function resolveToolDirectory(options = {}) {
  if (options.configPath) {
    return join(dirname(options.configPath), "tools");
  }

  if (options.project) {
    return join(process.cwd(), ".opencode", "tools");
  }

  const configDir = process.env.OPENCODE_CONFIG_DIR || join(homedir(), ".config", "opencode");
  return join(configDir, "tools");
}

export function resolveToolFilePath(options = {}) {
  return join(resolveToolDirectory(options), DEFAULT_TOOL_ASSET_NAME);
}

export async function installLocalTool(options = {}) {
  const toolUrl = options.toolUrl || DEFAULT_TOOL_URL;
  const toolFilePath = resolveToolFilePath(options);
  const nextText = await downloadToolAsset(toolUrl);

  mkdirSync(resolveToolDirectory(options), { recursive: true });

  const currentText = existsSync(toolFilePath) ? readFileSync(toolFilePath, "utf8") : null;
  if (currentText === nextText) {
    return {
      changed: false,
      toolFilePath,
      toolUrl,
      backupPath: null,
    };
  }

  const backupPath = currentText === null ? null : backupExistingFile(toolFilePath, currentText);
  writeFileSync(toolFilePath, ensureTrailingEol(nextText), "utf8");

  return {
    changed: true,
    toolFilePath,
    toolUrl,
    backupPath,
  };
}

export function uninstallLocalTool(options = {}) {
  const toolFilePath = resolveToolFilePath(options);
  if (!existsSync(toolFilePath)) {
    return {
      changed: false,
      toolFilePath,
      backupPath: null,
    };
  }

  const originalText = readFileSync(toolFilePath, "utf8");
  const backupPath = backupExistingFile(toolFilePath, originalText);
  rmSync(toolFilePath, { force: true });

  return {
    changed: true,
    toolFilePath,
    backupPath,
  };
}

async function downloadToolAsset(toolUrl) {
  if (toolUrl.startsWith("file://")) {
    return readFileSync(new URL(toolUrl), "utf8");
  }

  if (toolUrl.startsWith("/") && existsSync(toolUrl)) {
    return readFileSync(toolUrl, "utf8");
  }

  const response = await fetch(toolUrl);
  if (!response.ok) {
    throw new Error(`Unable to download tool asset from ${toolUrl}: HTTP ${response.status}`);
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

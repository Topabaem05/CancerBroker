import { applyEdits, modify, parse } from "jsonc-parser/lib/esm/main.js";
import { existsSync, mkdirSync, readFileSync, renameSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { homedir } from "node:os";

export const DEFAULT_PLUGIN_PACKAGE_NAME =
  process.env.OPENCODE_SESSION_MEMORY_PACKAGE || "opencode-session-memory-sidebar";

export function resolveTargetConfigPath(options = {}) {
  if (options.configPath) {
    return options.configPath;
  }

  if (options.project) {
    return join(process.cwd(), "opencode.json");
  }

  const configDir = process.env.OPENCODE_CONFIG_DIR || join(homedir(), ".config", "opencode");
  return join(configDir, "opencode.json");
}

export function installPluginIntoConfig(configPath, pluginName = DEFAULT_PLUGIN_PACKAGE_NAME) {
  const originalText = readConfigText(configPath);
  const parsed = parseJsoncObject(originalText, configPath);
  const existingEntries = Array.isArray(parsed.plugin) ? parsed.plugin : [];

  if (existingEntries.some((entry) => pluginEntryMatches(entry, pluginName))) {
    return {
      changed: false,
      configPath,
      backupPath: null,
      pluginName,
    };
  }

  const nextEntries = [...existingEntries, pluginName];
  return writeUpdatedConfig({
    configPath,
    originalText,
    pluginPath: ["plugin"],
    nextValue: nextEntries,
    pluginName,
  });
}

export function uninstallPluginFromConfig(configPath, pluginName = DEFAULT_PLUGIN_PACKAGE_NAME) {
  const originalText = readConfigText(configPath);
  const parsed = parseJsoncObject(originalText, configPath);
  const existingEntries = Array.isArray(parsed.plugin) ? parsed.plugin : [];
  const nextEntries = existingEntries.filter((entry) => !pluginEntryMatches(entry, pluginName));

  if (nextEntries.length === existingEntries.length) {
    return {
      changed: false,
      configPath,
      backupPath: null,
      pluginName,
    };
  }

  return writeUpdatedConfig({
    configPath,
    originalText,
    pluginPath: ["plugin"],
    nextValue: nextEntries.length > 0 ? nextEntries : undefined,
    pluginName,
  });
}

function writeUpdatedConfig(input) {
  const formattingOptions = detectFormattingOptions(input.originalText);
  const edits = modify(input.originalText, input.pluginPath, input.nextValue, { formattingOptions });
  const nextText = applyEdits(input.originalText, edits);
  parseJsoncObject(nextText, input.configPath);

  mkdirSync(dirname(input.configPath), { recursive: true });
  const backupPath = existsSync(input.configPath) ? createBackup(input.configPath, input.originalText) : null;
  atomicWrite(input.configPath, ensureTrailingEol(nextText, formattingOptions.eol));

  return {
    changed: true,
    configPath: input.configPath,
    backupPath,
    pluginName: input.pluginName,
  };
}

function readConfigText(configPath) {
  if (!existsSync(configPath)) {
    return "{}\n";
  }

  return readFileSync(configPath, "utf8");
}

function parseJsoncObject(text, configPath) {
  const errors = [];
  const parsed = parse(text, errors, {
    allowTrailingComma: true,
    disallowComments: false,
  });

  if (errors.length > 0 || !parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
    throw new Error(`Unable to parse OpenCode config at ${configPath}`);
  }

  return parsed;
}

function pluginEntryMatches(entry, pluginName) {
  if (typeof entry === "string") {
    return normalizePluginSpecifier(entry) === pluginName;
  }

  if (!entry || typeof entry !== "object") {
    return false;
  }

  if (typeof entry.plugin === "string") {
    return normalizePluginSpecifier(entry.plugin) === pluginName;
  }

  if (typeof entry.name === "string") {
    return normalizePluginSpecifier(entry.name) === pluginName;
  }

  return false;
}

function normalizePluginSpecifier(value) {
  if (!value.startsWith("@")) {
    const atIndex = value.indexOf("@");
    return atIndex === -1 ? value : value.slice(0, atIndex);
  }

  const slashIndex = value.indexOf("/");
  const versionAtIndex = value.indexOf("@", slashIndex + 1);
  return versionAtIndex === -1 ? value : value.slice(0, versionAtIndex);
}

function detectFormattingOptions(text) {
  const eol = text.includes("\r\n") ? "\r\n" : "\n";
  const indentMatch = text.match(/^[ \t]+(?=\S)/m);
  const indent = indentMatch ? indentMatch[0] : "  ";

  return {
    insertSpaces: !indent.includes("\t"),
    tabSize: indent.includes("\t") ? 1 : indent.length || 2,
    eol,
  };
}

function ensureTrailingEol(text, eol) {
  return text.endsWith(eol) ? text : `${text}${eol}`;
}

function createBackup(configPath, originalText) {
  const backupPath = `${configPath}.bak.${Date.now()}`;
  writeFileSync(backupPath, originalText, "utf8");
  return backupPath;
}

function atomicWrite(configPath, text) {
  const tempPath = `${configPath}.tmp.${process.pid}.${Date.now()}`;
  writeFileSync(tempPath, text, "utf8");
  renameSync(tempPath, configPath);
}

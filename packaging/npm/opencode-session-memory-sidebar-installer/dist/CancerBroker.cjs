#!/usr/bin/env node

// bin/config-file.mjs
var import_node_fs = require("node:fs");
var import_node_path = require("node:path");
var import_node_os = require("node:os");
var DEFAULT_PLUGIN_PACKAGE_NAME = process.env.OPENCODE_SESSION_MEMORY_PACKAGE || "opencode-session-memory-sidebar";
function resolveTargetConfigPath(options = {}) {
  if (options.configPath) {
    return options.configPath;
  }
  if (options.project) {
    return (0, import_node_path.join)(process.cwd(), "opencode.json");
  }
  const configDir = process.env.OPENCODE_CONFIG_DIR || (0, import_node_path.join)((0, import_node_os.homedir)(), ".config", "opencode");
  return (0, import_node_path.join)(configDir, "opencode.json");
}
function installPluginIntoConfig(configPath, pluginName = DEFAULT_PLUGIN_PACKAGE_NAME) {
  const originalText = readConfigText(configPath);
  const parsed = parseJsoncObject(originalText, configPath);
  const existingEntries = Array.isArray(parsed.plugin) ? parsed.plugin : [];
  if (existingEntries.some((entry) => pluginEntryMatches(entry, pluginName))) {
    return {
      changed: false,
      configPath,
      backupPath: null,
      pluginName
    };
  }
  const nextEntries = [...existingEntries, pluginName];
  return writeUpdatedConfig({
    configPath,
    originalText,
    pluginPath: ["plugin"],
    nextValue: nextEntries,
    pluginName
  });
}
function uninstallPluginFromConfig(configPath, pluginName = DEFAULT_PLUGIN_PACKAGE_NAME) {
  const originalText = readConfigText(configPath);
  const parsed = parseJsoncObject(originalText, configPath);
  const existingEntries = Array.isArray(parsed.plugin) ? parsed.plugin : [];
  const nextEntries = existingEntries.filter((entry) => !pluginEntryMatches(entry, pluginName));
  if (nextEntries.length === existingEntries.length) {
    return {
      changed: false,
      configPath,
      backupPath: null,
      pluginName
    };
  }
  return writeUpdatedConfig({
    configPath,
    originalText,
    pluginPath: ["plugin"],
    nextValue: nextEntries.length > 0 ? nextEntries : void 0,
    pluginName
  });
}
function writeUpdatedConfig(input) {
  const formattingOptions = detectFormattingOptions(input.originalText);
  const parsed = parseJsoncObject(input.originalText, input.configPath);
  if (input.nextValue === void 0) {
    delete parsed.plugin;
  } else {
    parsed.plugin = input.nextValue;
  }
  const nextText = serializeJsoncObject(parsed, formattingOptions);
  parseJsoncObject(nextText, input.configPath);
  (0, import_node_fs.mkdirSync)((0, import_node_path.dirname)(input.configPath), { recursive: true });
  const backupPath = (0, import_node_fs.existsSync)(input.configPath) ? createBackup(input.configPath, input.originalText) : null;
  atomicWrite(input.configPath, ensureTrailingEol(nextText, formattingOptions.eol));
  return {
    changed: true,
    configPath: input.configPath,
    backupPath,
    pluginName: input.pluginName
  };
}
function readConfigText(configPath) {
  if (!(0, import_node_fs.existsSync)(configPath)) {
    return "{}\n";
  }
  return (0, import_node_fs.readFileSync)(configPath, "utf8");
}
function parseJsoncObject(text, configPath) {
  const sanitized = sanitizeJsonc(text);
  let parsed;
  try {
    parsed = JSON.parse(sanitized);
  } catch {
    throw new Error(`Unable to parse OpenCode config at ${configPath}`);
  }
  if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
    throw new Error(`Unable to parse OpenCode config at ${configPath}`);
  }
  return parsed;
}
function serializeJsoncObject(value, formattingOptions) {
  const indent = formattingOptions.insertSpaces ? " ".repeat(formattingOptions.tabSize) : "	";
  const text = JSON.stringify(value, null, indent);
  return formattingOptions.eol === "\n" ? text : text.replace(/\n/g, formattingOptions.eol);
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
    insertSpaces: !indent.includes("	"),
    tabSize: indent.includes("	") ? 1 : indent.length || 2,
    eol
  };
}
function sanitizeJsonc(text) {
  const withoutComments = stripJsonComments(text);
  return stripTrailingCommas(withoutComments).trim() || "{}";
}
function stripJsonComments(text) {
  let result = "";
  let inString = false;
  let quote = '"';
  let escaping = false;
  let lineComment = false;
  let blockComment = false;
  for (let index = 0; index < text.length; index += 1) {
    const char = text[index];
    const next = text[index + 1];
    if (lineComment) {
      if (char === "\n") {
        lineComment = false;
        result += char;
      }
      continue;
    }
    if (blockComment) {
      if (char === "*" && next === "/") {
        blockComment = false;
        index += 1;
      }
      continue;
    }
    if (inString) {
      result += char;
      if (escaping) {
        escaping = false;
      } else if (char === "\\") {
        escaping = true;
      } else if (char === quote) {
        inString = false;
      }
      continue;
    }
    if ((char === '"' || char === "'") && !inString) {
      inString = true;
      quote = char;
      result += char;
      continue;
    }
    if (char === "/" && next === "/") {
      lineComment = true;
      index += 1;
      continue;
    }
    if (char === "/" && next === "*") {
      blockComment = true;
      index += 1;
      continue;
    }
    result += char;
  }
  return result;
}
function stripTrailingCommas(text) {
  let result = "";
  let inString = false;
  let quote = '"';
  let escaping = false;
  for (let index = 0; index < text.length; index += 1) {
    const char = text[index];
    if (inString) {
      result += char;
      if (escaping) {
        escaping = false;
      } else if (char === "\\") {
        escaping = true;
      } else if (char === quote) {
        inString = false;
      }
      continue;
    }
    if (char === '"' || char === "'") {
      inString = true;
      quote = char;
      result += char;
      continue;
    }
    if (char === ",") {
      let nextIndex = index + 1;
      while (nextIndex < text.length && /\s/.test(text[nextIndex])) {
        nextIndex += 1;
      }
      if (text[nextIndex] === "}" || text[nextIndex] === "]") {
        continue;
      }
    }
    result += char;
  }
  return result;
}
function ensureTrailingEol(text, eol) {
  return text.endsWith(eol) ? text : `${text}${eol}`;
}
function createBackup(configPath, originalText) {
  const backupPath = `${configPath}.bak.${Date.now()}`;
  (0, import_node_fs.writeFileSync)(backupPath, originalText, "utf8");
  return backupPath;
}
function atomicWrite(configPath, text) {
  const tempPath = `${configPath}.tmp.${process.pid}.${Date.now()}`;
  (0, import_node_fs.writeFileSync)(tempPath, text, "utf8");
  (0, import_node_fs.renameSync)(tempPath, configPath);
}

// bin/plugin-file.mjs
var import_node_fs2 = require("node:fs");
var import_node_path2 = require("node:path");
var import_node_os2 = require("node:os");
var DEFAULT_PLUGIN_ASSET_NAME = "CancerBroker.plugin.js";
var DEFAULT_PLUGIN_URL = process.env.OPENCODE_SESSION_MEMORY_PLUGIN_URL || `https://github.com/Topabaem05/CancerBroker/releases/latest/download/${DEFAULT_PLUGIN_ASSET_NAME}`;
function resolvePluginDirectory(options = {}) {
  if (options.configPath) {
    return (0, import_node_path2.join)((0, import_node_path2.dirname)(options.configPath), "plugins");
  }
  if (options.project) {
    return (0, import_node_path2.join)(process.cwd(), ".opencode", "plugins");
  }
  const configDir = process.env.OPENCODE_CONFIG_DIR || (0, import_node_path2.join)((0, import_node_os2.homedir)(), ".config", "opencode");
  return (0, import_node_path2.join)(configDir, "plugins");
}
function resolvePluginFilePath(options = {}) {
  return (0, import_node_path2.join)(resolvePluginDirectory(options), DEFAULT_PLUGIN_ASSET_NAME);
}
function uninstallLocalPlugin(options = {}) {
  const pluginFilePath = resolvePluginFilePath(options);
  if (!(0, import_node_fs2.existsSync)(pluginFilePath)) {
    return {
      changed: false,
      pluginFilePath,
      backupPath: null
    };
  }
  const originalText = (0, import_node_fs2.readFileSync)(pluginFilePath, "utf8");
  const backupPath = backupExistingFile(pluginFilePath, originalText);
  return {
    changed: true,
    pluginFilePath,
    backupPath
  };
}
function backupExistingFile(filePath, originalText) {
  const backupPath = `${filePath}.bak.${Date.now()}`;
  (0, import_node_fs2.writeFileSync)(backupPath, originalText, "utf8");
  return backupPath;
}

// bin/tool-file.mjs
var import_node_fs3 = require("node:fs");
var import_node_path3 = require("node:path");
var import_node_os3 = require("node:os");
var DEFAULT_TOOL_ASSET_NAME = "session_memory.js";
var DEFAULT_TOOL_URL = process.env.OPENCODE_SESSION_MEMORY_TOOL_URL || `https://github.com/Topabaem05/CancerBroker/releases/latest/download/${DEFAULT_TOOL_ASSET_NAME}`;
function resolveToolDirectory(options = {}) {
  if (options.configPath) {
    return (0, import_node_path3.join)((0, import_node_path3.dirname)(options.configPath), "tools");
  }
  if (options.project) {
    return (0, import_node_path3.join)(process.cwd(), ".opencode", "tools");
  }
  const configDir = process.env.OPENCODE_CONFIG_DIR || (0, import_node_path3.join)((0, import_node_os3.homedir)(), ".config", "opencode");
  return (0, import_node_path3.join)(configDir, "tools");
}
function resolveToolFilePath(options = {}) {
  return (0, import_node_path3.join)(resolveToolDirectory(options), DEFAULT_TOOL_ASSET_NAME);
}
async function installLocalTool(options = {}) {
  const toolUrl = options.toolUrl || DEFAULT_TOOL_URL;
  const toolFilePath = resolveToolFilePath(options);
  const nextText = await downloadToolAsset(toolUrl);
  (0, import_node_fs3.mkdirSync)(resolveToolDirectory(options), { recursive: true });
  const currentText = (0, import_node_fs3.existsSync)(toolFilePath) ? (0, import_node_fs3.readFileSync)(toolFilePath, "utf8") : null;
  if (currentText === nextText) {
    return {
      changed: false,
      toolFilePath,
      toolUrl,
      backupPath: null
    };
  }
  const backupPath = currentText === null ? null : backupExistingFile2(toolFilePath, currentText);
  (0, import_node_fs3.writeFileSync)(toolFilePath, ensureTrailingEol2(nextText), "utf8");
  return {
    changed: true,
    toolFilePath,
    toolUrl,
    backupPath
  };
}
function uninstallLocalTool(options = {}) {
  const toolFilePath = resolveToolFilePath(options);
  if (!(0, import_node_fs3.existsSync)(toolFilePath)) {
    return {
      changed: false,
      toolFilePath,
      backupPath: null
    };
  }
  const originalText = (0, import_node_fs3.readFileSync)(toolFilePath, "utf8");
  const backupPath = backupExistingFile2(toolFilePath, originalText);
  (0, import_node_fs3.rmSync)(toolFilePath, { force: true });
  return {
    changed: true,
    toolFilePath,
    backupPath
  };
}
async function downloadToolAsset(toolUrl) {
  if (toolUrl.startsWith("file://")) {
    return (0, import_node_fs3.readFileSync)(new URL(toolUrl), "utf8");
  }
  if (toolUrl.startsWith("/") && (0, import_node_fs3.existsSync)(toolUrl)) {
    return (0, import_node_fs3.readFileSync)(toolUrl, "utf8");
  }
  const response = await fetch(toolUrl);
  if (!response.ok) {
    throw new Error(`Unable to download tool asset from ${toolUrl}: HTTP ${response.status}`);
  }
  return await response.text();
}
function backupExistingFile2(filePath, originalText) {
  const backupPath = `${filePath}.bak.${Date.now()}`;
  (0, import_node_fs3.writeFileSync)(backupPath, originalText, "utf8");
  return backupPath;
}
function ensureTrailingEol2(text) {
  return text.endsWith("\n") ? text : `${text}
`;
}

// bin/standalone.mjs
var args = process.argv.slice(2);
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
    const toolResult2 = uninstallLocalTool(flags);
    if (toolResult2.changed) {
      console.log(`[opencode-session-memory-sidebar] Removed local tool at ${toolResult2.toolFilePath}`);
      if (toolResult2.backupPath) {
        console.log(`[opencode-session-memory-sidebar] Backup: ${toolResult2.backupPath}`);
      }
    } else {
      console.log(`[opencode-session-memory-sidebar] Local tool not present at ${toolResult2.toolFilePath}`);
    }
    const legacyPluginCleanup2 = uninstallLocalPlugin(flags);
    if (legacyPluginCleanup2.changed) {
      console.log(`[opencode-session-memory-sidebar] Removed legacy plugin file at ${legacyPluginCleanup2.pluginFilePath}`);
      if (legacyPluginCleanup2.backupPath) {
        console.log(`[opencode-session-memory-sidebar] Backup: ${legacyPluginCleanup2.backupPath}`);
      }
    }
    const cleanupResult2 = uninstallPluginFromConfig(configPath, DEFAULT_PLUGIN_PACKAGE_NAME);
    if (cleanupResult2.changed) {
      console.log(`[opencode-session-memory-sidebar] Removed npm plugin entry from ${cleanupResult2.configPath}`);
      if (cleanupResult2.backupPath) {
        console.log(`[opencode-session-memory-sidebar] Backup: ${cleanupResult2.backupPath}`);
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
    configPath: void 0,
    packageName: void 0
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

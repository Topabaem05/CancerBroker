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
function installPluginIntoConfig(configPath2, pluginName2 = DEFAULT_PLUGIN_PACKAGE_NAME) {
  const originalText = readConfigText(configPath2);
  const parsed = parseJsoncObject(originalText, configPath2);
  const existingEntries = Array.isArray(parsed.plugin) ? parsed.plugin : [];
  if (existingEntries.some((entry) => pluginEntryMatches(entry, pluginName2))) {
    return {
      changed: false,
      configPath: configPath2,
      backupPath: null,
      pluginName: pluginName2
    };
  }
  const nextEntries = [...existingEntries, pluginName2];
  return writeUpdatedConfig({
    configPath: configPath2,
    originalText,
    pluginPath: ["plugin"],
    nextValue: nextEntries,
    pluginName: pluginName2
  });
}
function uninstallPluginFromConfig(configPath2, pluginName2 = DEFAULT_PLUGIN_PACKAGE_NAME) {
  const originalText = readConfigText(configPath2);
  const parsed = parseJsoncObject(originalText, configPath2);
  const existingEntries = Array.isArray(parsed.plugin) ? parsed.plugin : [];
  const nextEntries = existingEntries.filter((entry) => !pluginEntryMatches(entry, pluginName2));
  if (nextEntries.length === existingEntries.length) {
    return {
      changed: false,
      configPath: configPath2,
      backupPath: null,
      pluginName: pluginName2
    };
  }
  return writeUpdatedConfig({
    configPath: configPath2,
    originalText,
    pluginPath: ["plugin"],
    nextValue: nextEntries.length > 0 ? nextEntries : void 0,
    pluginName: pluginName2
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
function readConfigText(configPath2) {
  if (!(0, import_node_fs.existsSync)(configPath2)) {
    return "{}\n";
  }
  return (0, import_node_fs.readFileSync)(configPath2, "utf8");
}
function parseJsoncObject(text, configPath2) {
  const sanitized = sanitizeJsonc(text);
  let parsed;
  try {
    parsed = JSON.parse(sanitized);
  } catch {
    throw new Error(`Unable to parse OpenCode config at ${configPath2}`);
  }
  if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
    throw new Error(`Unable to parse OpenCode config at ${configPath2}`);
  }
  return parsed;
}
function serializeJsoncObject(value, formattingOptions) {
  const indent = formattingOptions.insertSpaces ? " ".repeat(formattingOptions.tabSize) : "	";
  const text = JSON.stringify(value, null, indent);
  return formattingOptions.eol === "\n" ? text : text.replace(/\n/g, formattingOptions.eol);
}
function pluginEntryMatches(entry, pluginName2) {
  if (typeof entry === "string") {
    return normalizePluginSpecifier(entry) === pluginName2;
  }
  if (!entry || typeof entry !== "object") {
    return false;
  }
  if (typeof entry.plugin === "string") {
    return normalizePluginSpecifier(entry.plugin) === pluginName2;
  }
  if (typeof entry.name === "string") {
    return normalizePluginSpecifier(entry.name) === pluginName2;
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
  let result2 = "";
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
        result2 += char;
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
      result2 += char;
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
      result2 += char;
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
    result2 += char;
  }
  return result2;
}
function stripTrailingCommas(text) {
  let result2 = "";
  let inString = false;
  let quote = '"';
  let escaping = false;
  for (let index = 0; index < text.length; index += 1) {
    const char = text[index];
    if (inString) {
      result2 += char;
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
      result2 += char;
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
    result2 += char;
  }
  return result2;
}
function ensureTrailingEol(text, eol) {
  return text.endsWith(eol) ? text : `${text}${eol}`;
}
function createBackup(configPath2, originalText) {
  const backupPath = `${configPath2}.bak.${Date.now()}`;
  (0, import_node_fs.writeFileSync)(backupPath, originalText, "utf8");
  return backupPath;
}
function atomicWrite(configPath2, text) {
  const tempPath = `${configPath2}.tmp.${process.pid}.${Date.now()}`;
  (0, import_node_fs.writeFileSync)(tempPath, text, "utf8");
  (0, import_node_fs.renameSync)(tempPath, configPath2);
}

// bin/standalone.mjs
var args = process.argv.slice(2);
var command = args[0] === "uninstall" ? "uninstall" : "install";
var parsedArgs = command === "uninstall" ? args.slice(1) : args;
var flags = parseFlags(parsedArgs);
var configPath = resolveTargetConfigPath(flags);
var pluginName = flags.packageName || DEFAULT_PLUGIN_PACKAGE_NAME;
if (command === "uninstall") {
  const result2 = uninstallPluginFromConfig(configPath, pluginName);
  if (result2.changed) {
    console.log(`[opencode-session-memory-sidebar] Removed plugin from ${result2.configPath}`);
    if (result2.backupPath) {
      console.log(`[opencode-session-memory-sidebar] Backup: ${result2.backupPath}`);
    }
  } else {
    console.log(`[opencode-session-memory-sidebar] Plugin not present in ${result2.configPath}`);
  }
  console.log("[opencode-session-memory-sidebar] Restart OpenCode: opencode --restart");
  process.exit(0);
}
var result = installPluginIntoConfig(configPath, pluginName);
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
  const flags2 = {
    project: false,
    configPath: void 0,
    packageName: void 0
  };
  for (let index = 0; index < argv.length; index += 1) {
    const value = argv[index];
    if (value === "--project") {
      flags2.project = true;
      continue;
    }
    if (value === "--config" && typeof argv[index + 1] === "string") {
      flags2.configPath = argv[index + 1];
      index += 1;
      continue;
    }
    if (value === "--package" && typeof argv[index + 1] === "string") {
      flags2.packageName = argv[index + 1];
      index += 1;
    }
  }
  return flags2;
}

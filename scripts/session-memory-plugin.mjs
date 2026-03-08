#!/usr/bin/env node

import { spawnSync } from "node:child_process";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const SCRIPT_DIR = dirname(fileURLToPath(import.meta.url));
const DISPLAY_COMMAND = "./session-memory-plugin";
const INSTALLER_DIR = resolve(
  SCRIPT_DIR,
  "../packaging/npm/opencode-session-memory-sidebar-installer/bin",
);

const args = process.argv.slice(2);
const { command, forwardedArgs, restart } = parseArgs(args);

if (command === "help") {
  printUsage();
  process.exit(0);
}

const installerScript =
  command === "remove"
    ? resolve(INSTALLER_DIR, "uninstall.mjs")
    : resolve(INSTALLER_DIR, "install.mjs");

const result = spawnSync(process.execPath, [installerScript, ...forwardedArgs], {
  stdio: "inherit",
});

if (typeof result.status === "number" && result.status !== 0) {
  process.exit(result.status);
}

if (result.error) {
  throw result.error;
}

if (restart) {
  const restartResult = spawnSync("opencode", ["--restart"], {
    stdio: "inherit",
  });

  if (typeof restartResult.status === "number" && restartResult.status !== 0) {
    process.exit(restartResult.status);
  }

  if (restartResult.error) {
    throw restartResult.error;
  }
}

function parseArgs(argv) {
  if (argv.length === 0) {
    return {
      command: "add",
      forwardedArgs: [],
      restart: false,
    };
  }

  const first = argv[0];
  if (first === "help" || first === "--help" || first === "-h") {
    return {
      command: "help",
      forwardedArgs: [],
      restart: false,
    };
  }

  let command = "add";
  let startIndex = 0;
  if (isCommand(first)) {
    command = normalizeCommand(first);
    startIndex = 1;
  }

  const forwardedArgs = [];
  let restart = false;
  for (let index = startIndex; index < argv.length; index += 1) {
    const value = argv[index];
    if (value === "--restart") {
      restart = true;
      continue;
    }
    forwardedArgs.push(value);
  }

  return {
    command,
    forwardedArgs,
    restart,
  };
}

function isCommand(value) {
  return ["add", "install", "remove", "uninstall"].includes(value);
}

function normalizeCommand(value) {
  if (value === "remove" || value === "uninstall") {
    return "remove";
  }
  return "add";
}

function printUsage() {
  console.log(`Usage:
  ${DISPLAY_COMMAND} add [--project] [--config <path>] [--package <name>] [--restart]
  ${DISPLAY_COMMAND} remove [--project] [--config <path>] [--package <name>] [--restart]

Examples:
  ${DISPLAY_COMMAND} add
  ${DISPLAY_COMMAND} add --project
  ${DISPLAY_COMMAND} remove
  ${DISPLAY_COMMAND} add --config /tmp/opencode.json --restart

Notes:
  - add/remove only register or unregister the plugin in opencode.json.
  - This wrapper delegates all config mutation to the installer package.
  - For a bare command in this repo shell, run: . ./scripts/dev-env.sh
  - --restart is optional and runs: opencode --restart`);
}

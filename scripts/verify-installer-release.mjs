#!/usr/bin/env node

import { createHash } from "node:crypto";
import { existsSync, readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const SCRIPT_DIR = dirname(fileURLToPath(import.meta.url));
const args = process.argv.slice(2);

const repoRoot = resolveRepoRoot(args);
const installerPackageJsonPath = resolve(
  repoRoot,
  "packaging/npm/opencode-session-memory-sidebar-installer/package.json",
);
const formulaPath = resolve(repoRoot, "Formula/opencode-session-memory-sidebar-installer.rb");
const standaloneAssetPath = resolve(
  repoRoot,
  "packaging/npm/opencode-session-memory-sidebar-installer/dist/CancerBroker.cjs",
);
const pluginAssetPath = resolve(
  repoRoot,
  "packaging/npm/opencode-session-memory-sidebar/dist/CancerBroker.plugin.js",
);
const docsPaths = [
  resolve(repoRoot, "packaging/npm/opencode-session-memory-sidebar-installer/README.md"),
  resolve(repoRoot, "packaging/npm/README.md"),
  resolve(repoRoot, "readme-pages/korean-plugin-guide.md"),
];
const workflowPath = resolve(repoRoot, ".github/workflows/release-installer-asset.yml");
const installScriptPath = resolve(repoRoot, "install/opencode-session-memory-sidebar.sh");

const installerPackage = JSON.parse(readFileSync(installerPackageJsonPath, "utf8"));
const version = installerPackage.version;
if (typeof version !== "string" || version.length === 0) {
  throw new Error("Installer package version is missing");
}

const expectedTag = readFlagValue(args, "--tag") ?? `CancerBroker-v${version}`;
const expectedUrl = `https://github.com/Topabaem05/CancerBroker/releases/download/${expectedTag}/CancerBroker.cjs`;
const expectedPluginUrl = `https://github.com/Topabaem05/CancerBroker/releases/download/${expectedTag}/CancerBroker.plugin.js`;

assertFileExists(standaloneAssetPath, "Standalone asset");
assertFileExists(pluginAssetPath, "Plugin asset");

const digest = hashFile(standaloneAssetPath);
const formulaText = readFileSync(formulaPath, "utf8");
assertIncludes(formulaText, `url "${expectedUrl}"`, `${formulaPath} release asset URL`);
assertIncludes(formulaText, `version "${version}"`, `${formulaPath} version`);
assertIncludes(formulaText, `sha256 "${digest}"`, `${formulaPath} sha256`);

for (const docPath of docsPaths) {
  const content = readFileSync(docPath, "utf8");
  assertIncludes(content, expectedUrl, `${docPath} versioned release URL`);
}

const workflowText = readFileSync(workflowPath, "utf8");
assertIncludes(workflowText, '- "CancerBroker-v*"', `${workflowPath} tag trigger`);
assertIncludes(workflowText, 'node ./dist/CancerBroker.cjs --config "$TMP_DIR/opencode.json"', `${workflowPath} smoke test install`);
assertIncludes(workflowText, 'gh release upload "${{ steps.release_tag.outputs.tag }}" packaging/npm/opencode-session-memory-sidebar-installer/dist/CancerBroker.cjs --clobber', `${workflowPath} upload command`);
assertIncludes(workflowText, 'gh release upload "${{ steps.release_tag.outputs.tag }}" packaging/npm/opencode-session-memory-sidebar/dist/CancerBroker.plugin.js --clobber', `${workflowPath} plugin upload command`);

const installScriptText = readFileSync(installScriptPath, "utf8");
assertIncludes(
  installScriptText,
  'https://github.com/$INSTALLER_REPO/releases/latest/download/CancerBroker.cjs',
  `${installScriptPath} latest asset URL`,
);

if (args.includes("--check-remote")) {
  await assertRemoteOk(expectedUrl);
  await assertRemoteOk(expectedPluginUrl);
}

process.stdout.write(
  [
    `Installer release verification passed for ${expectedTag}.`,
    `Version: ${version}`,
    `Asset URL: ${expectedUrl}`,
    `Plugin URL: ${expectedPluginUrl}`,
    `SHA256: ${digest}`,
  ].join("\n") + "\n",
);

function resolveRepoRoot(argv) {
  const repoRootValue = readFlagValue(argv, "--repo-root");
  return repoRootValue ? resolve(repoRootValue) : resolve(SCRIPT_DIR, "..");
}

function readFlagValue(argv, flagName) {
  const index = argv.indexOf(flagName);
  if (index === -1) {
    return null;
  }

  const value = argv[index + 1];
  if (!value) {
    throw new Error(`${flagName} requires a value`);
  }

  return value;
}

function assertFileExists(filePath, label) {
  if (!existsSync(filePath)) {
    throw new Error(`${label} missing at ${filePath}`);
  }
}

function hashFile(filePath) {
  const hash = createHash("sha256");
  hash.update(readFileSync(filePath));
  return hash.digest("hex");
}

function assertIncludes(content, expectedValue, label) {
  if (!content.includes(expectedValue)) {
    throw new Error(`Expected ${label} to include: ${expectedValue}`);
  }
}

async function assertRemoteOk(url) {
  const response = await fetch(url, { method: "HEAD", redirect: "follow" });
  if (!response.ok) {
    throw new Error(`Remote release asset check failed for ${url}: HTTP ${response.status}`);
  }
}

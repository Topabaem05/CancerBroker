#!/usr/bin/env node

import { createHash } from "node:crypto";
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

const SCRIPT_DIR = dirname(fileURLToPath(import.meta.url));

const args = process.argv.slice(2);
if (args.length === 0 || args.includes("--help") || args.includes("-h")) {
  printUsage();
  process.exit(args.length === 0 ? 1 : 0);
}

const version = args[0];
if (!/^\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$/.test(version)) {
  throw new Error(`Invalid version: ${version}`);
}

const repoRoot = resolveRepoRoot(args.slice(1));
const installerTag = `CancerBroker-v${version}`;
const releaseAssetUrl = `https://github.com/Topabaem05/CancerBroker/releases/download/${installerTag}/opencode-session-memory-sidebar-installer.cjs`;

const installerPackageJsonPath = resolve(repoRoot, "packaging/npm/opencode-session-memory-sidebar-installer/package.json");
const formulaPath = resolve(repoRoot, "Formula/opencode-session-memory-sidebar-installer.rb");
const docsPaths = [
  resolve(repoRoot, "packaging/npm/opencode-session-memory-sidebar-installer/README.md"),
  resolve(repoRoot, "packaging/npm/README.md"),
  resolve(repoRoot, "readme-pages/korean-plugin-guide.md"),
];

updateInstallerPackageJson(installerPackageJsonPath, version);
installInstallerDependencies(repoRoot);
buildStandaloneInstaller(repoRoot);

const standaloneAssetPath = resolve(
  repoRoot,
  "packaging/npm/opencode-session-memory-sidebar-installer/dist/opencode-session-memory-sidebar-installer.cjs",
);
const sha256 = hashFile(standaloneAssetPath);

updateFormula(formulaPath, version, releaseAssetUrl, sha256);
for (const docPath of docsPaths) {
  replaceReleaseAssetUrls(docPath, releaseAssetUrl);
}

process.stdout.write(
  [
    `Prepared installer release ${version}.`,
    `Tag: ${installerTag}`,
    `Asset URL: ${releaseAssetUrl}`,
    `SHA256: ${sha256}`,
    "Next steps:",
    `  git add packaging/npm/opencode-session-memory-sidebar-installer/package.json packaging/npm/opencode-session-memory-sidebar-installer/dist/opencode-session-memory-sidebar-installer.cjs Formula/opencode-session-memory-sidebar-installer.rb packaging/npm/opencode-session-memory-sidebar-installer/README.md packaging/npm/README.md readme-pages/korean-plugin-guide.md`,
    `  git commit -m \"Prepare installer release v${version}\"`,
    "  git push origin main",
    `  git tag ${installerTag}`,
    `  git push origin ${installerTag}`,
  ].join("\n") + "\n",
);

function resolveRepoRoot(argv) {
  const repoRootFlagIndex = argv.indexOf("--repo-root");
  if (repoRootFlagIndex >= 0) {
    const repoRootValue = argv[repoRootFlagIndex + 1];
    if (!repoRootValue) {
      throw new Error("--repo-root requires a value");
    }
    return resolve(repoRootValue);
  }

  return resolve(SCRIPT_DIR, "..");
}

function updateInstallerPackageJson(filePath, versionValue) {
  const packageJson = JSON.parse(readFileSync(filePath, "utf8"));
  packageJson.version = versionValue;
  writeFileSync(filePath, `${JSON.stringify(packageJson, null, 2)}\n`);
}

function installInstallerDependencies(repoRootValue) {
  runCommand(
    "bun",
    ["install", "--frozen-lockfile"],
    resolve(repoRootValue, "packaging/npm/opencode-session-memory-sidebar-installer"),
  );
}

function buildStandaloneInstaller(repoRootValue) {
  runCommand(
    "bun",
    ["run", "build:standalone"],
    resolve(repoRootValue, "packaging/npm/opencode-session-memory-sidebar-installer"),
  );
}

function hashFile(filePath) {
  const hash = createHash("sha256");
  hash.update(readFileSync(filePath));
  return hash.digest("hex");
}

function updateFormula(filePath, versionValue, assetUrl, digest) {
  let content = readFileSync(filePath, "utf8");
  content = content.replace(/url ".*"/, `url "${assetUrl}"`);
  content = content.replace(/version ".*"/, `version "${versionValue}"`);
  content = content.replace(/sha256 ".*"/, `sha256 "${digest}"`);
  writeFileSync(filePath, content);
}

function replaceReleaseAssetUrls(filePath, assetUrl) {
  const content = readFileSync(filePath, "utf8");
  const updated = content.replace(
    /https:\/\/github\.com\/Topabaem05\/CancerBroker\/releases\/download\/CancerBroker-v[^\s)`]+\/opencode-session-memory-sidebar-installer\.cjs/g,
    assetUrl,
  );
  writeFileSync(filePath, updated);
}

function runCommand(command, commandArgs, cwd) {
  const result = spawnSync(command, commandArgs, {
    cwd,
    stdio: "inherit",
  });
  if (typeof result.status === "number" && result.status !== 0) {
    process.exit(result.status);
  }
  if (result.error) {
    throw result.error;
  }
}

function printUsage() {
  process.stdout.write(
    "Usage: node ./scripts/prepare-installer-release.mjs <version> [--repo-root <path>]\n",
  );
}

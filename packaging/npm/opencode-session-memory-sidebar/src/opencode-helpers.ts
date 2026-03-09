import { execFile, type ExecFileException } from "node:child_process";
import { promisify } from "node:util";

const execFileAsync = promisify(execFile);

const DEFAULT_HELPER_STALE_THRESHOLD_SECONDS = 30;
const MAX_HELPER_SUMMARY_ROWS = 5;

export interface OpencodeProcessRecord {
  readonly pid: number;
  readonly ppid: number;
  readonly pgid: number;
  readonly state: string;
  readonly elapsedSeconds: number;
  readonly rssBytes: number;
  readonly command: string;
  readonly args: string;
}

export interface OpencodeHelperRow {
  readonly pid: number;
  readonly ppid: number;
  readonly pgid: number;
  readonly elapsedSeconds: number;
  readonly rssBytes: number;
  readonly label: string;
}

export interface OpencodeHelperSummary {
  readonly activeCount: number;
  readonly activeTotalBytes: number;
  readonly activeRows: readonly OpencodeHelperRow[];
  readonly cleanupKilledCount: number;
  readonly cleanupSkippedCount: number;
}

export interface CollectOpencodeHelperSummaryInput {
  readonly currentPid?: number;
  readonly staleThresholdSeconds?: number;
  readonly sampleProcesses?: () => Promise<readonly OpencodeProcessRecord[]>;
  readonly signalProcess?: (pid: number, signal?: NodeJS.Signals | number) => void;
  readonly isProcessAlive?: (pid: number) => boolean;
}

export async function collectOpencodeHelperSummary(
  input: CollectOpencodeHelperSummaryInput = {},
): Promise<OpencodeHelperSummary> {
  const currentPid = input.currentPid ?? process.pid;
  const sampleProcesses = input.sampleProcesses ?? sampleMacOsProcesses;
  const records = await sampleProcesses();
  const root = resolveCurrentOpencodeRoot(records, currentPid);

  if (!root) {
    return {
      activeCount: 0,
      activeTotalBytes: 0,
      activeRows: [],
      cleanupKilledCount: 0,
      cleanupSkippedCount: 0,
    };
  }

  const activeHelpers = records.filter(
    (record) =>
      record.pid !== root.pid &&
      isTrackedHelperProcess(record) &&
      belongsToOpencodeRoot(records, record, root),
  );
  const cleanupCandidates = collectCleanupCandidates(activeHelpers, {
    staleThresholdSeconds: input.staleThresholdSeconds ?? DEFAULT_HELPER_STALE_THRESHOLD_SECONDS,
  });
  const cleanupResult = cleanupProcesses(cleanupCandidates, {
    signalProcess: input.signalProcess,
    isProcessAlive: input.isProcessAlive,
  });

  const activeRows = [...activeHelpers]
    .sort((left, right) => right.rssBytes - left.rssBytes)
    .slice(0, MAX_HELPER_SUMMARY_ROWS)
    .map((record) => ({
      pid: record.pid,
      ppid: record.ppid,
      pgid: record.pgid,
      elapsedSeconds: record.elapsedSeconds,
      rssBytes: record.rssBytes,
      label: describeHelper(record),
    }));

  return {
    activeCount: activeHelpers.length,
    activeTotalBytes: activeHelpers.reduce((sum, record) => sum + record.rssBytes, 0),
    activeRows,
    cleanupKilledCount: cleanupResult.killedCount,
    cleanupSkippedCount: cleanupResult.skippedCount,
  };
}

export async function sampleMacOsProcesses(): Promise<readonly OpencodeProcessRecord[]> {
  const { stdout } = await execFileAsync(
    "ps",
    ["-Ao", "pid=,ppid=,pgid=,state=,etime=,rss=,comm=,args="],
    {
      encoding: "utf8",
      timeout: 2000,
      maxBuffer: 512 * 1024,
      env: {
        ...process.env,
        LC_ALL: "C",
        LANG: "C",
      },
    },
  ).catch((error) => {
    throw normalizeExecError(error);
  });

  return stdout
    .split(/\r?\n/)
    .map((line) => parseProcessRow(line))
    .filter((row): row is OpencodeProcessRecord => row !== null);
}

export function parseProcessRow(line: string): OpencodeProcessRecord | null {
  const trimmed = line.trim();
  if (!trimmed) {
    return null;
  }

  const match = /^([0-9]+)\s+([0-9]+)\s+([0-9]+)\s+(\S+)\s+(\S+)\s+([0-9]+)\s+(\S+)\s+(.+)$/.exec(trimmed);
  if (!match) {
    return null;
  }

  const pid = Number.parseInt(match[1], 10);
  const ppid = Number.parseInt(match[2], 10);
  const pgid = Number.parseInt(match[3], 10);
  const state = match[4];
  const elapsedSeconds = parseElapsedSeconds(match[5]);
  const rssKib = Number.parseInt(match[6], 10);
  const command = match[7];
  const args = match[8];

  if (
    !Number.isInteger(pid) ||
    !Number.isInteger(ppid) ||
    !Number.isInteger(pgid) ||
    !Number.isInteger(elapsedSeconds) ||
    !Number.isInteger(rssKib)
  ) {
    return null;
  }

  return {
    pid,
    ppid,
    pgid,
    state,
    elapsedSeconds,
    rssBytes: rssKib * 1024,
    command,
    args,
  };
}

export function parseElapsedSeconds(value: string): number {
  const daySplit = value.split("-");
  const days = daySplit.length === 2 ? Number.parseInt(daySplit[0], 10) : 0;
  const timePart = daySplit.length === 2 ? daySplit[1] : daySplit[0];
  const units = timePart.split(":").map((part) => Number.parseInt(part, 10));

  if (units.some((part) => !Number.isInteger(part) || part < 0) || !Number.isInteger(days) || days < 0) {
    return 0;
  }

  if (units.length === 2) {
    return days * 86400 + units[0] * 60 + units[1];
  }

  if (units.length === 3) {
    return days * 86400 + units[0] * 3600 + units[1] * 60 + units[2];
  }

  return 0;
}

function resolveCurrentOpencodeRoot(
  records: readonly OpencodeProcessRecord[],
  currentPid: number,
): OpencodeProcessRecord | null {
  const byPid = new Map(records.map((record) => [record.pid, record]));
  const visited = new Set<number>();
  let pid: number | undefined = currentPid;

  while (pid && !visited.has(pid)) {
    visited.add(pid);
    const record = byPid.get(pid);
    if (!record) {
      break;
    }
    if (isOpencodeProcess(record)) {
      return record;
    }
    pid = record.ppid;
  }

  return null;
}

function belongsToOpencodeRoot(
  records: readonly OpencodeProcessRecord[],
  record: OpencodeProcessRecord,
  root: OpencodeProcessRecord,
): boolean {
  if (record.pgid === root.pgid) {
    return true;
  }

  const byPid = new Map(records.map((row) => [row.pid, row]));
  const visited = new Set<number>();
  let pid: number | undefined = record.ppid;

  while (pid && !visited.has(pid)) {
    visited.add(pid);
    if (pid === root.pid) {
      return true;
    }
    const parent = byPid.get(pid);
    if (!parent) {
      break;
    }
    pid = parent.ppid;
  }

  return false;
}

function isOpencodeProcess(record: OpencodeProcessRecord): boolean {
  return basename(record.command) === "opencode" || record.args.startsWith("opencode ") || record.args === "opencode";
}

function isTrackedHelperProcess(record: OpencodeProcessRecord): boolean {
  const combined = `${record.command} ${record.args}`.toLowerCase();
  const commandBase = basename(record.command);
  return (
    commandBase === "node" ||
    commandBase === "biome" ||
    combined.includes("typescript-language-server") ||
    combined.includes("tsserver.js") ||
    combined.includes("typingsinstaller.js") ||
    combined.includes("biome lsp-proxy") ||
    combined.includes("context7-mcp") ||
    combined.includes("tokscale")
  );
}

function collectCleanupCandidates(
  records: readonly OpencodeProcessRecord[],
  input: { readonly staleThresholdSeconds: number },
): OpencodeProcessRecord[] {
  const bySignature = new Map<string, OpencodeProcessRecord[]>();

  for (const record of records) {
    const signature = `${basename(record.command)}|${record.args}`;
    const list = bySignature.get(signature) ?? [];
    list.push(record);
    bySignature.set(signature, list);
  }

  const candidates: OpencodeProcessRecord[] = [];
  for (const group of bySignature.values()) {
    if (group.length < 2) {
      continue;
    }

    const ordered = [...group].sort((left, right) => left.elapsedSeconds - right.elapsedSeconds);
    const survivors = ordered.slice(0, 1);
    const stale = ordered.slice(survivors.length).filter((record) => record.elapsedSeconds >= input.staleThresholdSeconds);
    candidates.push(...stale.filter((record) => !record.state.startsWith("Z")));
  }

  return candidates;
}

function cleanupProcesses(
  records: readonly OpencodeProcessRecord[],
  input: {
    readonly signalProcess?: (pid: number, signal?: NodeJS.Signals | number) => void;
    readonly isProcessAlive?: (pid: number) => boolean;
  },
): { killedCount: number; skippedCount: number } {
  const signalProcess = input.signalProcess ?? process.kill.bind(process);
  const isProcessAlive = input.isProcessAlive ?? ((pid: number) => isAlive(pid));
  let killedCount = 0;
  let skippedCount = 0;

  for (const record of records) {
    try {
      signalProcess(record.pid, "SIGTERM");
      if (isProcessAlive(record.pid)) {
        signalProcess(record.pid, "SIGKILL");
      }
      killedCount += 1;
    } catch {
      skippedCount += 1;
    }
  }

  return { killedCount, skippedCount };
}

function isAlive(pid: number): boolean {
  try {
    process.kill(pid, 0);
    return true;
  } catch {
    return false;
  }
}

function describeHelper(record: OpencodeProcessRecord): string {
  const combined = record.args.toLowerCase();
  if (combined.includes("typescript-language-server")) {
    return "typescript-language-server";
  }
  if (combined.includes("tsserver.js")) {
    return "tsserver";
  }
  if (combined.includes("typingsinstaller.js")) {
    return "typingsInstaller";
  }
  if (combined.includes("biome")) {
    return "biome";
  }
  if (combined.includes("context7-mcp")) {
    return "context7-mcp";
  }
  if (combined.includes("tokscale")) {
    return "tokscale";
  }
  return basename(record.command);
}

function basename(value: string): string {
  const normalized = value.replace(/\\/g, "/");
  const parts = normalized.split("/");
  return parts[parts.length - 1] || normalized;
}

function normalizeExecError(error: unknown): Error {
  const failure = error as ExecFileException & { stderr?: string | Buffer };
  const stderr = failure.stderr ? String(failure.stderr) : "";
  const message = stderr.trim() || failure.message || "Unable to inspect macOS process tree";
  return new Error(message);
}

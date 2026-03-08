import { execFile, type ExecFileException } from "node:child_process";
import { promisify } from "node:util";

const execFileAsync = promisify(execFile);

export const DEFAULT_PROCESS_SAMPLE_TIMEOUT_MS = 1000;
export const MAX_PROCESS_SAMPLE_TIMEOUT_MS = 5000;

export type PsCommandFailureState =
  | "not_found"
  | "permission_denied"
  | "timeout"
  | "command_error";

export type PsCommandResult =
  | {
      readonly state: "ok";
      readonly stdout: string;
      readonly stderr: string;
    }
  | {
      readonly state: PsCommandFailureState;
      readonly stdout: string;
      readonly stderr: string;
      readonly detail?: string;
    };

export interface ProcessIdentity {
  readonly pid: number;
  readonly startTimeIso: string;
}

export interface ParsedPsSample {
  readonly pid: number;
  readonly startTimeIso: string;
  readonly rssBytes: number;
}

export interface MacOsProcessSampled {
  readonly state: "sampled";
  readonly metric: "rss_bytes";
  readonly bytes: number;
  readonly identity: ProcessIdentity;
  readonly sampledAtIso: string;
}

export interface MacOsProcessSamplingUnavailable {
  readonly state:
    | "not_found"
    | "permission_denied"
    | "timeout"
    | "parse_failure"
    | "command_error";
  readonly pid: number;
  readonly sampledAtIso: string;
  readonly detail?: string;
}

export type MacOsProcessSample = MacOsProcessSampled | MacOsProcessSamplingUnavailable;

export type RunPsCommand = (pid: number, timeoutMs: number) => Promise<PsCommandResult>;

export interface SampleMacOsProcessInput {
  readonly pid: number;
  readonly timeoutMs?: number;
  readonly now?: () => Date;
  readonly runPsCommand?: RunPsCommand;
}

const PS_SAMPLE_PATTERN =
  /^\s*(\d+)\s+([A-Za-z]{3}\s+[A-Za-z]{3}\s+\d{1,2}\s+\d{2}:\d{2}:\d{2}\s+\d{4})\s+(\d+)\s*$/;

export function resolveProcessSampleTimeoutMs(timeoutMs?: number): number {
  if (!Number.isFinite(timeoutMs) || timeoutMs === undefined || timeoutMs <= 0) {
    return DEFAULT_PROCESS_SAMPLE_TIMEOUT_MS;
  }

  const rounded = Math.floor(timeoutMs);
  return Math.max(1, Math.min(MAX_PROCESS_SAMPLE_TIMEOUT_MS, rounded));
}

export function createProcessIdentityKey(identity: ProcessIdentity): string {
  return `${identity.pid}:${identity.startTimeIso}`;
}

export function isSameProcessIdentity(a: ProcessIdentity, b: ProcessIdentity): boolean {
  return a.pid === b.pid && a.startTimeIso === b.startTimeIso;
}

export function parsePsSampleOutput(stdout: string): ParsedPsSample | null {
  const firstLine = stdout
    .split(/\r?\n/)
    .map((line) => line.trim())
    .find((line) => line.length > 0);

  if (!firstLine) {
    return null;
  }

  const match = PS_SAMPLE_PATTERN.exec(firstLine);
  if (!match) {
    return null;
  }

  const pid = Number.parseInt(match[1], 10);
  const startTimeIso = parsePsStartTimeToIso(match[2]);
  const rssKib = Number.parseInt(match[3], 10);

  if (!Number.isInteger(pid) || pid <= 0 || !startTimeIso || !Number.isFinite(rssKib) || rssKib < 0) {
    return null;
  }

  const rssBytes = rssKib * 1024;
  if (!Number.isSafeInteger(rssBytes)) {
    return null;
  }

  return {
    pid,
    startTimeIso,
    rssBytes,
  };
}

export async function sampleMacOsProcess(input: SampleMacOsProcessInput): Promise<MacOsProcessSample> {
  const now = input.now ?? (() => new Date());
  const sampledAtIso = now().toISOString();

  if (!Number.isInteger(input.pid) || input.pid <= 0) {
    return {
      state: "parse_failure",
      pid: input.pid,
      sampledAtIso,
      detail: "pid must be a positive integer",
    };
  }

  const timeoutMs = resolveProcessSampleTimeoutMs(input.timeoutMs);
  const runPsCommand = input.runPsCommand ?? runPsCommandWithExecFile;
  const commandResult = await runPsCommand(input.pid, timeoutMs);

  if (commandResult.state !== "ok") {
    return {
      state: commandResult.state,
      pid: input.pid,
      sampledAtIso,
      detail: commandResult.detail,
    };
  }

  if (isPermissionDeniedText(commandResult.stderr)) {
    return {
      state: "permission_denied",
      pid: input.pid,
      sampledAtIso,
      detail: normalizeDetail(commandResult.stderr),
    };
  }

  const parsed = parsePsSampleOutput(commandResult.stdout);
  if (!parsed || parsed.pid !== input.pid) {
    const detail = normalizeDetail(commandResult.stderr);
    return {
      state: "parse_failure",
      pid: input.pid,
      sampledAtIso,
      detail,
    };
  }

  return {
    state: "sampled",
    metric: "rss_bytes",
    bytes: parsed.rssBytes,
    identity: {
      pid: parsed.pid,
      startTimeIso: parsed.startTimeIso,
    },
    sampledAtIso,
  };
}

export async function runPsCommandWithExecFile(
  pid: number,
  timeoutMs: number,
): Promise<PsCommandResult> {
  try {
    const { stdout, stderr } = await execFileAsync(
      "ps",
      ["-o", "pid=", "-o", "lstart=", "-o", "rss=", "-p", String(pid)],
      {
        encoding: "utf8",
        timeout: timeoutMs,
        maxBuffer: 64 * 1024,
        env: {
          ...process.env,
          LC_ALL: "C",
          LANG: "C",
        },
      },
    );

    return {
      state: "ok",
      stdout: toText(stdout),
      stderr: toText(stderr),
    };
  } catch (error) {
    return mapExecFileFailure(error);
  }
}

function parsePsStartTimeToIso(startTimeText: string): string | null {
  const normalized = startTimeText.replace(/\s+/g, " ").trim();
  const parsedMs = Date.parse(normalized);

  if (!Number.isFinite(parsedMs)) {
    return null;
  }

  return new Date(parsedMs).toISOString();
}

function mapExecFileFailure(error: unknown): Extract<PsCommandResult, { state: PsCommandFailureState }> {
  const failure = error as ExecFileException & {
    stdout?: string | Buffer;
    stderr?: string | Buffer;
  };

  const stdout = toText(failure.stdout);
  const stderr = toText(failure.stderr);
  const detail = normalizeDetail(failure.message);
  const combined = `${detail ?? ""}\n${stderr}`;

  if (failure.code === "ETIMEDOUT" || failure.killed) {
    return {
      state: "timeout",
      stdout,
      stderr,
      detail,
    };
  }

  if (isPermissionDeniedText(combined)) {
    return {
      state: "permission_denied",
      stdout,
      stderr,
      detail,
    };
  }

  if (isNotFoundText(combined) || isEmptyResultForMissingPid(failure, stdout, stderr)) {
    return {
      state: "not_found",
      stdout,
      stderr,
      detail,
    };
  }

  return {
    state: "command_error",
    stdout,
    stderr,
    detail,
  };
}

function toText(value: unknown): string {
  if (typeof value === "string") {
    return value;
  }

  if (value instanceof Buffer) {
    return value.toString("utf8");
  }

  if (value === undefined || value === null) {
    return "";
  }

  return String(value);
}

function normalizeDetail(value: unknown): string | undefined {
  const normalized = toText(value).trim();
  return normalized.length > 0 ? normalized : undefined;
}

function isPermissionDeniedText(value: string): boolean {
  const normalized = value.toLowerCase();
  return normalized.includes("operation not permitted") || normalized.includes("permission denied");
}

function isNotFoundText(value: string): boolean {
  const normalized = value.toLowerCase();
  return normalized.includes("no such process") || normalized.includes("not found");
}

function isEmptyResultForMissingPid(
  failure: ExecFileException,
  stdout: string,
  stderr: string,
): boolean {
  return (
    typeof failure.code === "number" &&
    failure.code !== 0 &&
    stdout.trim().length === 0 &&
    stderr.trim().length === 0
  );
}

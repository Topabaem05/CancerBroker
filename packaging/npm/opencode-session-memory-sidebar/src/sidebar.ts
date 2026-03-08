import { probeRuntimeCapabilities, type RuntimeCapabilityProbeInput } from "./capabilities";
import {
  discoverLiveSessions,
  type LiveSessionRecord,
  type SessionApiLike,
} from "./live-sessions";
import {
  attributeSessionRam,
  type AttributeSessionRamInput,
  type SessionProcessMappingCandidate,
} from "./ram-attribution";
import { aggregateSessionTokenUsage } from "./token-usage";
import type {
  CapabilityDisabledState,
  RuntimeCapabilityState,
  SessionMemoryRow,
  SessionMemorySummary,
  SessionRamState,
} from "./types";

export const SIDEBAR_PANEL_TITLE = "Session Memory";
export const SIDEBAR_POLL_INTERVAL_MS = 5000;
export const FALLBACK_UNAVAILABLE_LABEL = "unavailable_unknown";

type Awaitable<T> = Promise<T> | T;

export interface SidebarItemView {
  readonly id: string;
  readonly label: string;
  readonly value?: string;
}

export interface SidebarRowView {
  readonly sessionId: string;
  readonly isCurrent: boolean;
  readonly tokensTotal: number;
  readonly ramLabel: string;
}

export interface SidebarPanelModel {
  readonly title: typeof SIDEBAR_PANEL_TITLE;
  readonly pollIntervalMs: typeof SIDEBAR_POLL_INTERVAL_MS;
  readonly capability: RuntimeCapabilityState;
  readonly summary: SessionMemorySummary;
  readonly current: SidebarRowView | null;
  readonly others: readonly SidebarRowView[];
}

export interface SessionMemorySnapshot {
  readonly capability: RuntimeCapabilityState;
  readonly rows: readonly SessionMemoryRow[];
}

export interface SidebarRuntimeItem {
  readonly id: string;
  readonly label: string;
  readonly value?: string;
}

export interface SidebarRuntimeDefinition {
  readonly id: string;
  readonly title: string;
  readonly items: () => Awaitable<readonly SidebarRuntimeItem[]>;
}

export interface BuildSidebarSnapshotInput {
  readonly capabilityProbeInput: RuntimeCapabilityProbeInput;
  readonly sessionApi: SessionApiLike;
  readonly currentSessionId?: string;
  readonly collectRows?: (input: {
    readonly sessionApi: SessionApiLike;
    readonly currentSessionId?: string;
  }) => Promise<SessionMemoryRow[]>;
}

export async function buildSessionMemorySnapshot(
  input: BuildSidebarSnapshotInput,
): Promise<SessionMemorySnapshot> {
  const capability = probeRuntimeCapabilities(input.capabilityProbeInput);
  if (capability.state === "disabled") {
    return {
      capability,
      rows: [],
    };
  }

  const collectRows = input.collectRows ?? collectSessionMemoryRows;
  const rows = await collectRows({
    sessionApi: input.sessionApi,
    currentSessionId: input.currentSessionId,
  });

  return {
    capability,
    rows,
  };
}

export function buildSidebarPanelModel(snapshot: SessionMemorySnapshot): SidebarPanelModel {
  const orderedRows = sortRowsCurrentFirst(snapshot.rows);
  const summary = summarizeRows(orderedRows);
  const current = orderedRows.find((row) => row.isCurrent) ?? null;
  const others = orderedRows.filter((row) => !row.isCurrent);

  return {
    title: SIDEBAR_PANEL_TITLE,
    pollIntervalMs: SIDEBAR_POLL_INTERVAL_MS,
    capability: snapshot.capability,
    summary,
    current: current ? toSidebarRowView(current) : null,
    others: others.map(toSidebarRowView),
  };
}

export function buildSidebarItems(model: SidebarPanelModel): SidebarItemView[] {
  const items: SidebarItemView[] = [];

  items.push({ id: "summary.live", label: "Live", value: String(model.summary.liveSessionCount) });
  items.push({
    id: "summary.exact",
    label: "Exact RAM",
    value: `${model.summary.exactRamCoverageCount}/${model.summary.liveSessionCount}`,
  });
  items.push({
    id: "summary.total",
    label: "Exact Total",
    value: formatBytes(model.summary.exactRamTotalBytes),
  });
  items.push({
    id: "summary.unavailable",
    label: "Unavailable",
    value: String(model.summary.unavailableRamCount),
  });

  if (model.capability.state === "disabled") {
    items.push(...disabledItems(model.capability));
    return items;
  }

  if (model.current) {
    items.push({
      id: `current.${model.current.sessionId}`,
      label: `Current ${model.current.sessionId}`,
      value: `tokens ${model.current.tokensTotal} | RAM ${model.current.ramLabel}`,
    });
  } else {
    items.push({
      id: "current.none",
      label: "Current",
      value: "none",
    });
  }

  for (const row of model.others) {
    items.push({
      id: `other.${row.sessionId}`,
      label: `Other ${row.sessionId}`,
      value: `tokens ${row.tokensTotal} | RAM ${row.ramLabel}`,
    });
  }

  return items;
}

export function createSessionMemorySidebarDefinition(input: {
  readonly id?: string;
  readonly snapshot: () => Promise<SessionMemorySnapshot>;
}): SidebarRuntimeDefinition {
  return {
    id: input.id ?? "session-memory",
    title: SIDEBAR_PANEL_TITLE,
    items: async () => {
      const snapshot = await input.snapshot();
      const model = buildSidebarPanelModel(snapshot);
      return buildSidebarItems(model);
    },
  };
}

export async function collectSessionMemoryRows(input: {
  readonly sessionApi: SessionApiLike;
  readonly currentSessionId?: string;
  readonly ramAttributionOverrides?: Pick<
    AttributeSessionRamInput,
    "sampleTimeoutMs" | "now" | "sampleProcess" | "previousExactSamplesBySessionId"
  >;
}): Promise<SessionMemoryRow[]> {
  const liveSessions = await discoverLiveSessions({
    sessionApi: input.sessionApi,
    currentSessionId: input.currentSessionId,
  });

  const messageResults = await Promise.all(
    liveSessions.map(async (session) => ({
      sessionId: session.sessionId,
      messages: await readSessionMessages(input.sessionApi, session.sessionId),
    })),
  );

  const sessionMappings = await Promise.all(
    liveSessions.map((session) => resolveProcessMappingCandidate(input.sessionApi, session)),
  );
  const ramRows = await attributeSessionRam({
    sessions: sessionMappings,
    ...input.ramAttributionOverrides,
  });
  const ramBySessionId = new Map(ramRows.map((row) => [row.sessionId, row.ram]));
  const messagesBySessionId = new Map(messageResults.map((row) => [row.sessionId, row.messages]));

  return liveSessions.map((session) => {
    const messages = messagesBySessionId.get(session.sessionId) ?? [];
    return {
      sessionId: session.sessionId,
      isCurrent: session.isCurrent,
      tokenUsage: aggregateSessionTokenUsage(messages),
      ram:
        ramBySessionId.get(session.sessionId) ?? {
          mappingState: "unavailable",
          reason: "unavailable_no_pid",
          sampledAtIso: new Date().toISOString(),
        },
    };
  });
}

async function readSessionMessages(sessionApi: SessionApiLike, sessionId: string): Promise<unknown[]> {
  const calls = [
    () => sessionApi.messages({ sessionId }),
    () => sessionApi.messages({ id: sessionId }),
    () => sessionApi.messages(sessionId),
  ];

  for (const call of calls) {
    try {
      const result = await call();
      const normalized = normalizeMessageArray(result);
      if (normalized.length > 0 || Array.isArray(result)) {
        return normalized;
      }
    } catch {
      continue;
    }
  }

  return [];
}

function normalizeMessageArray(raw: unknown): unknown[] {
  if (Array.isArray(raw)) {
    return raw;
  }

  if (!raw || typeof raw !== "object") {
    return [];
  }

  const candidate = raw as Record<string, unknown>;
  if (Array.isArray(candidate.messages)) {
    return candidate.messages;
  }
  if (Array.isArray(candidate.items)) {
    return candidate.items;
  }
  if (Array.isArray(candidate.data)) {
    return candidate.data;
  }
  return [];
}

async function resolveProcessMappingCandidate(
  sessionApi: SessionApiLike,
  session: LiveSessionRecord,
): Promise<SessionProcessMappingCandidate> {
  const fallback: SessionProcessMappingCandidate = {
    sessionId: session.sessionId,
  };

  const calls = [
    () => sessionApi.get({ sessionId: session.sessionId }),
    () => sessionApi.get({ id: session.sessionId }),
    () => sessionApi.get(session.sessionId),
  ];

  for (const call of calls) {
    try {
      const detail = await call();
      const mapping = extractMappingFromSessionDetail(detail, session.sessionId);
      if (mapping.pid) {
        return mapping;
      }
    } catch {
      continue;
    }
  }

  return fallback;
}

function extractMappingFromSessionDetail(detail: unknown, sessionId: string): SessionProcessMappingCandidate {
  const records = flattenRecords(detail);

  let pid: number | undefined;
  let startTimeIso: string | undefined;
  for (const record of records) {
    pid = pid ?? firstFiniteInt(record, ["pid", "processPid", "process_pid"]);
    startTimeIso =
      startTimeIso ??
      firstString(record, [
        "startTimeIso",
        "startedAtIso",
        "start_time_iso",
        "startTime",
        "startedAt",
      ]);
  }

  return {
    sessionId,
    pid,
    startTimeIso,
  };
}

function flattenRecords(raw: unknown): Record<string, unknown>[] {
  if (!raw || typeof raw !== "object") {
    return [];
  }

  const queue: unknown[] = [raw];
  const output: Record<string, unknown>[] = [];

  while (queue.length > 0) {
    const current = queue.shift();
    if (!current || typeof current !== "object") {
      continue;
    }

    const record = current as Record<string, unknown>;
    output.push(record);

    for (const value of Object.values(record)) {
      if (value && typeof value === "object") {
        queue.push(value);
      }
    }
  }

  return output;
}

function firstFiniteInt(record: Record<string, unknown>, keys: readonly string[]): number | undefined {
  for (const key of keys) {
    const value = record[key];
    if (typeof value === "number" && Number.isFinite(value) && Number.isInteger(value) && value > 0) {
      return value;
    }
  }

  return undefined;
}

function firstString(record: Record<string, unknown>, keys: readonly string[]): string | undefined {
  for (const key of keys) {
    const value = record[key];
    if (typeof value === "string" && value.length > 0) {
      return value;
    }
  }

  return undefined;
}

function summarizeRows(rows: readonly SessionMemoryRow[]): SessionMemorySummary {
  let exactRamCoverageCount = 0;
  let exactRamTotalBytes = 0;
  let unavailableRamCount = 0;

  for (const row of rows) {
    if (row.ram.mappingState === "exact") {
      exactRamCoverageCount += 1;
      exactRamTotalBytes += row.ram.bytes;
      continue;
    }
    unavailableRamCount += 1;
  }

  return {
    liveSessionCount: rows.length,
    exactRamCoverageCount,
    unavailableRamCount,
    exactRamTotalBytes,
  };
}

function sortRowsCurrentFirst(rows: readonly SessionMemoryRow[]): SessionMemoryRow[] {
  return [...rows].sort((left, right) => {
    if (left.isCurrent && !right.isCurrent) {
      return -1;
    }
    if (!left.isCurrent && right.isCurrent) {
      return 1;
    }
    return left.sessionId.localeCompare(right.sessionId);
  });
}

function toSidebarRowView(row: SessionMemoryRow): SidebarRowView {
  return {
    sessionId: row.sessionId,
    isCurrent: row.isCurrent,
    tokensTotal: row.tokenUsage.totalTokens,
    ramLabel: ramStateToLabel(row.ram),
  };
}

function ramStateToLabel(ram: SessionRamState): string {
  if (ram.mappingState === "exact") {
    return formatBytes(ram.bytes);
  }

  return `unavailable (${ram.reason ?? FALLBACK_UNAVAILABLE_LABEL})`;
}

function disabledItems(disabled: CapabilityDisabledState): SidebarItemView[] {
  return [
    {
      id: "disabled.reason",
      label: "Disabled",
      value: disabled.reason,
    },
    {
      id: "disabled.message",
      label: "Reason",
      value: disabled.message,
    },
  ];
}

function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) {
    return "0 B";
  }

  const units = ["B", "KiB", "MiB", "GiB", "TiB"];
  let value = bytes;
  let unitIndex = 0;
  while (value >= 1024 && unitIndex < units.length - 1) {
    value /= 1024;
    unitIndex += 1;
  }

  const precision = value >= 100 ? 0 : value >= 10 ? 1 : 2;
  return `${value.toFixed(precision)} ${units[unitIndex]}`;
}

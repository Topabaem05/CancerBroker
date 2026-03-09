type Awaitable<T> = T | Promise<T>;

export interface SessionApiLike {
  readonly list: (input?: unknown) => Awaitable<unknown>;
  readonly get: (input?: unknown) => Awaitable<unknown>;
  readonly messages: (input?: unknown) => Awaitable<unknown>;
}

export interface LiveSessionRecord {
  readonly sessionId: string;
  readonly status: string;
  readonly isCurrent: boolean;
  readonly projectId?: string;
  readonly projectPath?: string;
}

export interface DiscoverLiveSessionsInput {
  readonly sessionApi: SessionApiLike;
  readonly currentSessionId?: string;
}

export interface VisibleSessionSummary {
  readonly storedSessionCount: number;
  readonly liveSessionCount: number;
  readonly currentSessionId?: string;
}

const INACTIVE_SESSION_STATUSES = new Set([
  "inactive",
  "closed",
  "deleted",
  "historical",
  "archived",
  "terminated",
  "completed",
  "failed",
  "error",
]);

export async function discoverLiveSessions(input: DiscoverLiveSessionsInput): Promise<LiveSessionRecord[]> {
  const sessions = normalizeSessionList(await listSessionsAcrossRuntime(input.sessionApi));
  const currentSessionId = await resolveCurrentSessionId(input);

  const liveRows = sessions
    .filter((session) => isLiveStatus(session.status))
    .map((session) => ({
      sessionId: session.id,
      status: session.status,
      isCurrent: currentSessionId === session.id,
      projectId: session.projectId,
      projectPath: session.projectPath,
    }));

  return stablePinCurrentFirst(liveRows);
}

export async function summarizeVisibleSessions(
  input: DiscoverLiveSessionsInput,
): Promise<VisibleSessionSummary> {
  const sessions = normalizeSessionList(await listSessionsAcrossRuntime(input.sessionApi));
  const currentSessionId = await resolveCurrentSessionId(input);
  const liveSessionCount = sessions.filter((session) => isLiveStatus(session.status)).length;

  return {
    storedSessionCount: sessions.length,
    liveSessionCount,
    currentSessionId,
  };
}

interface NormalizedSession {
  readonly id: string;
  readonly status: string;
  readonly projectId?: string;
  readonly projectPath?: string;
}

function normalizeSessionList(raw: unknown): NormalizedSession[] {
  const payload = unwrapSessionArray(raw);
  const normalized = payload
    .map((item) => normalizeSession(item))
    .filter((item): item is NormalizedSession => item !== null);

  return dedupeById(normalized);
}

function unwrapSessionArray(raw: unknown): unknown[] {
  if (Array.isArray(raw)) {
    return raw;
  }

  if (!raw || typeof raw !== "object") {
    return [];
  }

  const candidate = raw as Record<string, unknown>;
  if (Array.isArray(candidate.sessions)) {
    return candidate.sessions;
  }
  if (Array.isArray(candidate.items)) {
    return candidate.items;
  }
  if (Array.isArray(candidate.data)) {
    return candidate.data;
  }

  return [];
}

function normalizeSession(raw: unknown): NormalizedSession | null {
  if (!raw || typeof raw !== "object") {
    return null;
  }

  const candidate = raw as Record<string, unknown>;
  const id = asString(candidate.id) ?? asString(candidate.sessionId) ?? asString(candidate.session_id);
  if (!id) {
    return null;
  }

  return {
    id,
    status: normalizeStatus(candidate.status),
    projectId: asString(candidate.projectId) ?? asString(candidate.project_id),
    projectPath: asString(candidate.projectPath) ?? asString(candidate.project_path),
  };
}

function normalizeStatus(raw: unknown): string {
  if (typeof raw === "string") {
    return raw.trim().toLowerCase();
  }

  if (!raw || typeof raw !== "object") {
    return "unknown";
  }

  const candidate = raw as Record<string, unknown>;
  const nested = asString(candidate.type) ?? asString(candidate.state) ?? asString(candidate.status);
  return nested ? nested.trim().toLowerCase() : "unknown";
}

function isLiveStatus(status: string): boolean {
  return !INACTIVE_SESSION_STATUSES.has(status);
}

async function listSessionsAcrossRuntime(sessionApi: SessionApiLike): Promise<unknown> {
  const listCalls = [
    () => sessionApi.list({ scope: "all", status: "live" }),
    () => sessionApi.list({ scope: "all", status: "open" }),
    () => sessionApi.list({ all: true }),
    () => sessionApi.list(),
  ];

  for (const call of listCalls) {
    try {
      const value = await call();
      const asArray = unwrapSessionArray(value);
      if (asArray.length > 0) {
        return value;
      }
      if (Array.isArray(value)) {
        return value;
      }
    } catch {
      continue;
    }
  }

  return [];
}

async function resolveCurrentSessionId(input: DiscoverLiveSessionsInput): Promise<string | undefined> {
  if (input.currentSessionId) {
    return input.currentSessionId;
  }

  try {
    const current = await input.sessionApi.get();
    return normalizeCurrentSessionId(current);
  } catch {
    return undefined;
  }
}

function normalizeCurrentSessionId(raw: unknown): string | undefined {
  if (!raw || typeof raw !== "object") {
    return undefined;
  }

  const candidate = raw as Record<string, unknown>;
  if (asString(candidate.id)) {
    return asString(candidate.id);
  }
  if (asString(candidate.sessionId)) {
    return asString(candidate.sessionId);
  }
  if (asString(candidate.session_id)) {
    return asString(candidate.session_id);
  }

  if (candidate.session && typeof candidate.session === "object") {
    const nested = candidate.session as Record<string, unknown>;
    return asString(nested.id) ?? asString(nested.sessionId) ?? asString(nested.session_id);
  }

  return undefined;
}

function stablePinCurrentFirst(rows: LiveSessionRecord[]): LiveSessionRecord[] {
  return [...rows].sort((left, right) => {
    if (left.isCurrent && !right.isCurrent) {
      return -1;
    }
    if (!left.isCurrent && right.isCurrent) {
      return 1;
    }

    const projectCompare = (left.projectPath ?? left.projectId ?? "").localeCompare(
      right.projectPath ?? right.projectId ?? "",
    );
    if (projectCompare !== 0) {
      return projectCompare;
    }

    return left.sessionId.localeCompare(right.sessionId);
  });
}

function asString(value: unknown): string | undefined {
  return typeof value === "string" && value.length > 0 ? value : undefined;
}

function dedupeById(input: NormalizedSession[]): NormalizedSession[] {
  const seen = new Set<string>();
  const output: NormalizedSession[] = [];

  for (const session of input) {
    if (seen.has(session.id)) {
      continue;
    }
    seen.add(session.id);
    output.push(session);
  }

  return output;
}

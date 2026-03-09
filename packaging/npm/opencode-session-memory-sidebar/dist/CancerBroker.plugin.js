// src/capabilities.ts
var DISABLED_REASON_MESSAGES = {
  platform_unsupported: "Session Memory supports macOS only in v1 (platform_unsupported).",
  sidebar_unsupported: "This OpenCode runtime does not expose a usable sidebar hook (sidebar_unsupported).",
  session_api_unavailable: "Required OpenCode session APIs are unreachable in this runtime (session_api_unavailable)."
};
function probeRuntimeCapabilities(input = {}) {
  const platform = input.platform ?? process.platform;
  if (platform !== "darwin") {
    return {
      state: "disabled",
      reason: "platform_unsupported",
      message: DISABLED_REASON_MESSAGES.platform_unsupported
    };
  }
  if (!hasUsableSidebarHook(input.sidebarHook)) {
    return {
      state: "disabled",
      reason: "sidebar_unsupported",
      message: DISABLED_REASON_MESSAGES.sidebar_unsupported
    };
  }
  if (!hasUsableSessionApi(input.sessionApi)) {
    return {
      state: "disabled",
      reason: "session_api_unavailable",
      message: DISABLED_REASON_MESSAGES.session_api_unavailable
    };
  }
  return {
    state: "enabled",
    platform: "darwin",
    v1RamMetric: "rss_bytes"
  };
}
function hasUsableSidebarHook(sidebarHook) {
  if (typeof sidebarHook === "function") {
    return true;
  }
  if (!sidebarHook || typeof sidebarHook !== "object") {
    return false;
  }
  const candidate = sidebarHook;
  return typeof candidate.items === "function" || typeof candidate.create === "function" || typeof candidate.register === "function";
}
function hasUsableSessionApi(sessionApi) {
  if (!sessionApi || typeof sessionApi !== "object") {
    return false;
  }
  const candidate = sessionApi;
  return typeof candidate.list === "function" && typeof candidate.get === "function" && typeof candidate.messages === "function";
}

// src/live-sessions.ts
var INACTIVE_SESSION_STATUSES = /* @__PURE__ */ new Set([
  "inactive",
  "closed",
  "deleted",
  "historical",
  "archived",
  "terminated",
  "completed",
  "failed",
  "error"
]);
async function discoverLiveSessions(input) {
  const sessions = normalizeSessionList(await listSessionsAcrossRuntime(input.sessionApi));
  const currentSessionId = await resolveCurrentSessionId(input);
  const liveRows = sessions.filter((session) => isLiveStatus(session.status)).map((session) => ({
    sessionId: session.id,
    status: session.status,
    isCurrent: currentSessionId === session.id,
    projectId: session.projectId,
    projectPath: session.projectPath
  }));
  return stablePinCurrentFirst(liveRows);
}
function normalizeSessionList(raw) {
  const payload = unwrapSessionArray(raw);
  const normalized = payload.map((item) => normalizeSession(item)).filter((item) => item !== null);
  return dedupeById(normalized);
}
function unwrapSessionArray(raw) {
  if (Array.isArray(raw)) {
    return raw;
  }
  if (!raw || typeof raw !== "object") {
    return [];
  }
  const candidate = raw;
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
function normalizeSession(raw) {
  if (!raw || typeof raw !== "object") {
    return null;
  }
  const candidate = raw;
  const id = asString(candidate.id) ?? asString(candidate.sessionId) ?? asString(candidate.session_id);
  if (!id) {
    return null;
  }
  return {
    id,
    status: normalizeStatus(candidate.status),
    projectId: asString(candidate.projectId) ?? asString(candidate.project_id),
    projectPath: asString(candidate.projectPath) ?? asString(candidate.project_path)
  };
}
function normalizeStatus(raw) {
  if (typeof raw === "string") {
    return raw.trim().toLowerCase();
  }
  if (!raw || typeof raw !== "object") {
    return "unknown";
  }
  const candidate = raw;
  const nested = asString(candidate.type) ?? asString(candidate.state) ?? asString(candidate.status);
  return nested ? nested.trim().toLowerCase() : "unknown";
}
function isLiveStatus(status) {
  return !INACTIVE_SESSION_STATUSES.has(status);
}
async function listSessionsAcrossRuntime(sessionApi) {
  const listCalls = [
    () => sessionApi.list({ scope: "all", status: "live" }),
    () => sessionApi.list({ scope: "all", status: "open" }),
    () => sessionApi.list({ all: true }),
    () => sessionApi.list()
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
async function resolveCurrentSessionId(input) {
  if (input.currentSessionId) {
    return input.currentSessionId;
  }
  try {
    const current = await input.sessionApi.get();
    return normalizeCurrentSessionId(current);
  } catch {
    return void 0;
  }
}
function normalizeCurrentSessionId(raw) {
  if (!raw || typeof raw !== "object") {
    return void 0;
  }
  const candidate = raw;
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
    const nested = candidate.session;
    return asString(nested.id) ?? asString(nested.sessionId) ?? asString(nested.session_id);
  }
  return void 0;
}
function stablePinCurrentFirst(rows) {
  return [...rows].sort((left, right) => {
    if (left.isCurrent && !right.isCurrent) {
      return -1;
    }
    if (!left.isCurrent && right.isCurrent) {
      return 1;
    }
    const projectCompare = (left.projectPath ?? left.projectId ?? "").localeCompare(
      right.projectPath ?? right.projectId ?? ""
    );
    if (projectCompare !== 0) {
      return projectCompare;
    }
    return left.sessionId.localeCompare(right.sessionId);
  });
}
function asString(value) {
  return typeof value === "string" && value.length > 0 ? value : void 0;
}
function dedupeById(input) {
  const seen = /* @__PURE__ */ new Set();
  const output = [];
  for (const session of input) {
    if (seen.has(session.id)) {
      continue;
    }
    seen.add(session.id);
    output.push(session);
  }
  return output;
}

// src/process-macos.ts
import { execFile } from "node:child_process";
import { promisify } from "node:util";
var execFileAsync = promisify(execFile);
var DEFAULT_PROCESS_SAMPLE_TIMEOUT_MS = 1e3;
var MAX_PROCESS_SAMPLE_TIMEOUT_MS = 5e3;
var PS_SAMPLE_PATTERN = /^\s*(\d+)\s+([A-Za-z]{3}\s+[A-Za-z]{3}\s+\d{1,2}\s+\d{2}:\d{2}:\d{2}\s+\d{4})\s+(\d+)\s*$/;
function resolveProcessSampleTimeoutMs(timeoutMs) {
  if (!Number.isFinite(timeoutMs) || timeoutMs === void 0 || timeoutMs <= 0) {
    return DEFAULT_PROCESS_SAMPLE_TIMEOUT_MS;
  }
  const rounded = Math.floor(timeoutMs);
  return Math.max(1, Math.min(MAX_PROCESS_SAMPLE_TIMEOUT_MS, rounded));
}
function parsePsSampleOutput(stdout) {
  const firstLine = stdout.split(/\r?\n/).map((line) => line.trim()).find((line) => line.length > 0);
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
    rssBytes
  };
}
async function sampleMacOsProcess(input) {
  const now = input.now ?? (() => /* @__PURE__ */ new Date());
  const sampledAtIso = now().toISOString();
  if (!Number.isInteger(input.pid) || input.pid <= 0) {
    return {
      state: "parse_failure",
      pid: input.pid,
      sampledAtIso,
      detail: "pid must be a positive integer"
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
      detail: commandResult.detail
    };
  }
  if (isPermissionDeniedText(commandResult.stderr)) {
    return {
      state: "permission_denied",
      pid: input.pid,
      sampledAtIso,
      detail: normalizeDetail(commandResult.stderr)
    };
  }
  const parsed = parsePsSampleOutput(commandResult.stdout);
  if (!parsed || parsed.pid !== input.pid) {
    const detail = normalizeDetail(commandResult.stderr);
    return {
      state: "parse_failure",
      pid: input.pid,
      sampledAtIso,
      detail
    };
  }
  return {
    state: "sampled",
    metric: "rss_bytes",
    bytes: parsed.rssBytes,
    identity: {
      pid: parsed.pid,
      startTimeIso: parsed.startTimeIso
    },
    sampledAtIso
  };
}
async function runPsCommandWithExecFile(pid, timeoutMs) {
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
          LANG: "C"
        }
      }
    );
    return {
      state: "ok",
      stdout: toText(stdout),
      stderr: toText(stderr)
    };
  } catch (error) {
    return mapExecFileFailure(error);
  }
}
function parsePsStartTimeToIso(startTimeText) {
  const normalized = startTimeText.replace(/\s+/g, " ").trim();
  const parsedMs = Date.parse(normalized);
  if (!Number.isFinite(parsedMs)) {
    return null;
  }
  return new Date(parsedMs).toISOString();
}
function mapExecFileFailure(error) {
  const failure = error;
  const stdout = toText(failure.stdout);
  const stderr = toText(failure.stderr);
  const detail = normalizeDetail(failure.message);
  const combined = `${detail ?? ""}
${stderr}`;
  if (failure.code === "ETIMEDOUT" || failure.killed) {
    return {
      state: "timeout",
      stdout,
      stderr,
      detail
    };
  }
  if (isPermissionDeniedText(combined)) {
    return {
      state: "permission_denied",
      stdout,
      stderr,
      detail
    };
  }
  if (isNotFoundText(combined) || isEmptyResultForMissingPid(failure, stdout, stderr)) {
    return {
      state: "not_found",
      stdout,
      stderr,
      detail
    };
  }
  return {
    state: "command_error",
    stdout,
    stderr,
    detail
  };
}
function toText(value) {
  if (typeof value === "string") {
    return value;
  }
  if (value instanceof Buffer) {
    return value.toString("utf8");
  }
  if (value === void 0 || value === null) {
    return "";
  }
  return String(value);
}
function normalizeDetail(value) {
  const normalized = toText(value).trim();
  return normalized.length > 0 ? normalized : void 0;
}
function isPermissionDeniedText(value) {
  const normalized = value.toLowerCase();
  return normalized.includes("operation not permitted") || normalized.includes("permission denied");
}
function isNotFoundText(value) {
  const normalized = value.toLowerCase();
  return normalized.includes("no such process") || normalized.includes("not found");
}
function isEmptyResultForMissingPid(failure, stdout, stderr) {
  return typeof failure.code === "number" && failure.code !== 0 && stdout.trim().length === 0 && stderr.trim().length === 0;
}

// src/types.ts
function createRamState(input) {
  if (input.mappingState === "exact") {
    if (!Number.isFinite(input.bytes) || input.bytes < 0) {
      throw new Error("exact RAM state requires a non-negative finite byte value");
    }
    return {
      mappingState: "exact",
      bytes: input.bytes,
      metric: input.metric ?? "rss_bytes",
      sampledAtIso: input.sampledAtIso
    };
  }
  if ("bytes" in input && typeof input.bytes === "number") {
    throw new Error("numeric RAM is only valid when mappingState is exact");
  }
  return {
    mappingState: "unavailable",
    reason: input.reason,
    sampledAtIso: input.sampledAtIso
  };
}

// src/ram-attribution.ts
async function attributeSessionRam(input) {
  const now = input.now ?? (() => /* @__PURE__ */ new Date());
  const fallbackSampledAtIso = now().toISOString();
  const sampleTimeoutMs = resolveProcessSampleTimeoutMs(input.sampleTimeoutMs);
  const sampleProcess = input.sampleProcess ?? sampleMacOsProcess;
  const sharedPids = collectSharedPids(input.sessions);
  const sampleByPid = /* @__PURE__ */ new Map();
  const rows = [];
  for (const session of input.sessions) {
    const pid = session.pid;
    if (!isUsablePid(pid)) {
      rows.push({
        sessionId: session.sessionId,
        ram: createRamState({
          mappingState: "unavailable",
          reason: "unavailable_no_pid",
          sampledAtIso: fallbackSampledAtIso
        })
      });
      continue;
    }
    const sessionWithPid = {
      ...session,
      pid
    };
    if (sharedPids.has(pid)) {
      rows.push({
        sessionId: session.sessionId,
        ram: createRamState({
          mappingState: "unavailable",
          reason: "unavailable_shared_process",
          sampledAtIso: fallbackSampledAtIso
        })
      });
      continue;
    }
    const expectedStartTimeIso = normalizeStartTimeIso(session.startTimeIso);
    if (!expectedStartTimeIso) {
      rows.push({
        sessionId: session.sessionId,
        ram: createRamState({
          mappingState: "unavailable",
          reason: "stale",
          sampledAtIso: pickStaleTimestamp({
            session: sessionWithPid,
            fallbackSampledAtIso,
            previousExactSamplesBySessionId: input.previousExactSamplesBySessionId
          })
        })
      });
      continue;
    }
    const cachedSamplePromise = sampleByPid.get(pid);
    const samplePromise = cachedSamplePromise ?? sampleProcess({
      pid,
      timeoutMs: sampleTimeoutMs,
      now
    });
    if (!cachedSamplePromise) {
      sampleByPid.set(pid, samplePromise);
    }
    const sample = await samplePromise;
    rows.push({
      sessionId: session.sessionId,
      ram: classifySampleForSession({
        session: sessionWithPid,
        sample,
        expectedStartTimeIso,
        previousExactSamplesBySessionId: input.previousExactSamplesBySessionId
      })
    });
  }
  return rows;
}
function classifySampleForSession(input) {
  if (input.sample.state === "sampled") {
    const observedStartTimeIso = normalizeStartTimeIso(input.sample.identity.startTimeIso);
    const isSameInstance = input.sample.identity.pid === input.session.pid && observedStartTimeIso === input.expectedStartTimeIso;
    if (isSameInstance) {
      return createRamState({
        mappingState: "exact",
        metric: input.sample.metric,
        bytes: input.sample.bytes,
        sampledAtIso: input.sample.sampledAtIso
      });
    }
    return createRamState({
      mappingState: "unavailable",
      reason: "stale",
      sampledAtIso: pickStaleTimestamp({
        session: input.session,
        expectedStartTimeIso: input.expectedStartTimeIso,
        fallbackSampledAtIso: input.sample.sampledAtIso,
        previousExactSamplesBySessionId: input.previousExactSamplesBySessionId
      })
    });
  }
  if (input.sample.state === "permission_denied") {
    return createRamState({
      mappingState: "unavailable",
      reason: "permission_denied",
      sampledAtIso: input.sample.sampledAtIso
    });
  }
  return createRamState({
    mappingState: "unavailable",
    reason: "stale",
    sampledAtIso: pickStaleTimestamp({
      session: input.session,
      expectedStartTimeIso: input.expectedStartTimeIso,
      fallbackSampledAtIso: input.sample.sampledAtIso,
      previousExactSamplesBySessionId: input.previousExactSamplesBySessionId
    })
  });
}
function isUsablePid(pid) {
  return typeof pid === "number" && Number.isInteger(pid) && pid > 0;
}
function normalizeStartTimeIso(startTimeIso) {
  if (!startTimeIso) {
    return null;
  }
  const parsed = Date.parse(startTimeIso);
  if (!Number.isFinite(parsed)) {
    return null;
  }
  return new Date(parsed).toISOString();
}
function collectSharedPids(sessions) {
  const countsByPid = /* @__PURE__ */ new Map();
  for (const session of sessions) {
    if (!isUsablePid(session.pid)) {
      continue;
    }
    countsByPid.set(session.pid, (countsByPid.get(session.pid) ?? 0) + 1);
  }
  return new Set(
    Array.from(countsByPid.entries()).filter(([, count]) => count > 1).map(([pid]) => pid)
  );
}
function pickStaleTimestamp(input) {
  const previous = input.previousExactSamplesBySessionId?.get(input.session.sessionId);
  if (!previous || previous.pid !== input.session.pid) {
    return input.fallbackSampledAtIso;
  }
  if (input.expectedStartTimeIso) {
    const previousStartTimeIso = normalizeStartTimeIso(previous.startTimeIso);
    if (previousStartTimeIso !== input.expectedStartTimeIso) {
      return input.fallbackSampledAtIso;
    }
  }
  return previous.sampledAtIso;
}

// src/token-usage.ts
var EMPTY_TOKEN_USAGE = {
  inputTokens: 0,
  outputTokens: 0,
  reasoningTokens: 0,
  cacheReadTokens: 0,
  totalTokens: 0
};
function aggregateSessionTokenUsage(messages) {
  let inputTokens = 0;
  let outputTokens = 0;
  let reasoningTokens = 0;
  let cacheReadTokens = 0;
  for (const message of messages) {
    if (!isAssistantMessage(message)) {
      continue;
    }
    const usage = extractUsageBuckets(message);
    inputTokens += usage.inputTokens;
    outputTokens += usage.outputTokens;
    reasoningTokens += usage.reasoningTokens;
    cacheReadTokens += usage.cacheReadTokens;
  }
  const totalTokens = inputTokens + outputTokens + reasoningTokens + cacheReadTokens;
  return {
    inputTokens,
    outputTokens,
    reasoningTokens,
    cacheReadTokens,
    totalTokens
  };
}
function isAssistantMessage(message) {
  if (!message || typeof message !== "object") {
    return false;
  }
  const candidate = message;
  const role = asString2(candidate.role) ?? asString2(candidate.type) ?? "";
  return role.toLowerCase() === "assistant";
}
function extractUsageBuckets(message) {
  const usageScopes = gatherUsageScopes(message);
  const inputTokens = readFirstNumber(usageScopes, [
    ["inputTokens"],
    ["input_tokens"],
    ["input"],
    ["promptTokens"],
    ["prompt_tokens"],
    ["prompt"]
  ]);
  const outputTokens = readFirstNumber(usageScopes, [
    ["outputTokens"],
    ["output_tokens"],
    ["output"],
    ["completionTokens"],
    ["completion_tokens"],
    ["completion"]
  ]);
  const reasoningTokens = readFirstNumber(usageScopes, [
    ["reasoningTokens"],
    ["reasoning_tokens"],
    ["reasoning"]
  ]);
  const cacheReadTokens = readFirstNumber(usageScopes, [
    ["cacheReadTokens"],
    ["cache_read_tokens"],
    ["cacheReadInputTokens"],
    ["cache_read_input_tokens"],
    ["cacheRead"],
    ["cache", "read"]
  ]);
  return {
    inputTokens,
    outputTokens,
    reasoningTokens,
    cacheReadTokens
  };
}
function gatherUsageScopes(message) {
  if (!message || typeof message !== "object") {
    return [];
  }
  const candidate = message;
  const scopes = [];
  if (isRecord(candidate)) {
    scopes.push(candidate);
  }
  if (isRecord(candidate.tokens)) {
    scopes.push(candidate.tokens);
  }
  if (isRecord(candidate.usage)) {
    scopes.push(candidate.usage);
  }
  if (isRecord(candidate.metrics)) {
    scopes.push(candidate.metrics);
    if (isRecord(candidate.metrics.tokens)) {
      scopes.push(candidate.metrics.tokens);
    }
  }
  return scopes;
}
function readFirstNumber(scopes, paths) {
  for (const path of paths) {
    for (const scope of scopes) {
      const value = readPath(scope, path);
      if (isFiniteNonNegativeNumber(value)) {
        return value;
      }
    }
  }
  return 0;
}
function readPath(record, path) {
  let current = record;
  for (const key of path) {
    if (!current || typeof current !== "object") {
      return void 0;
    }
    current = current[key];
  }
  return current;
}
function isFiniteNonNegativeNumber(value) {
  return typeof value === "number" && Number.isFinite(value) && value >= 0;
}
function asString2(value) {
  return typeof value === "string" && value.length > 0 ? value : void 0;
}
function isRecord(value) {
  return !!value && typeof value === "object";
}

// src/sidebar.ts
var SIDEBAR_PANEL_TITLE = "Session Memory";
var SIDEBAR_POLL_INTERVAL_MS = 5e3;
var FALLBACK_UNAVAILABLE_LABEL = "unavailable_unknown";
async function buildSessionMemorySnapshot(input) {
  const capability = probeRuntimeCapabilities(input.capabilityProbeInput);
  if (capability.state === "disabled") {
    return {
      capability,
      rows: []
    };
  }
  const collectRows = input.collectRows ?? collectSessionMemoryRows;
  const rows = await collectRows({
    sessionApi: input.sessionApi,
    currentSessionId: input.currentSessionId
  });
  return {
    capability,
    rows
  };
}
function buildSidebarPanelModel(snapshot) {
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
    others: others.map(toSidebarRowView)
  };
}
function buildSidebarItems(model) {
  const items = [];
  items.push({ id: "summary.live", label: "Live", value: String(model.summary.liveSessionCount) });
  items.push({
    id: "summary.exact",
    label: "Exact RAM",
    value: `${model.summary.exactRamCoverageCount}/${model.summary.liveSessionCount}`
  });
  items.push({
    id: "summary.total",
    label: "Exact Total",
    value: formatBytes(model.summary.exactRamTotalBytes)
  });
  items.push({
    id: "summary.unavailable",
    label: "Unavailable",
    value: String(model.summary.unavailableRamCount)
  });
  if (model.capability.state === "disabled") {
    items.push(...disabledItems(model.capability));
    return items;
  }
  if (model.current) {
    items.push({
      id: `current.${model.current.sessionId}`,
      label: `Current ${model.current.sessionId}`,
      value: `tokens ${model.current.tokensTotal} | RAM ${model.current.ramLabel}`
    });
  } else {
    items.push({
      id: "current.none",
      label: "Current",
      value: "none"
    });
  }
  for (const row of model.others) {
    items.push({
      id: `other.${row.sessionId}`,
      label: `Other ${row.sessionId}`,
      value: `tokens ${row.tokensTotal} | RAM ${row.ramLabel}`
    });
  }
  return items;
}
function createSessionMemorySidebarDefinition(input) {
  return {
    id: input.id ?? "session-memory",
    title: SIDEBAR_PANEL_TITLE,
    items: async () => {
      const snapshot = await input.snapshot();
      const model = buildSidebarPanelModel(snapshot);
      return buildSidebarItems(model);
    }
  };
}
async function collectSessionMemoryRows(input) {
  const liveSessions = await discoverLiveSessions({
    sessionApi: input.sessionApi,
    currentSessionId: input.currentSessionId
  });
  const messageResults = await Promise.all(
    liveSessions.map(async (session) => ({
      sessionId: session.sessionId,
      messages: await readSessionMessages(input.sessionApi, session.sessionId)
    }))
  );
  const sessionMappings = await Promise.all(
    liveSessions.map((session) => resolveProcessMappingCandidate(input.sessionApi, session))
  );
  const ramRows = await attributeSessionRam({
    sessions: sessionMappings,
    ...input.ramAttributionOverrides
  });
  const ramBySessionId = new Map(ramRows.map((row) => [row.sessionId, row.ram]));
  const messagesBySessionId = new Map(messageResults.map((row) => [row.sessionId, row.messages]));
  return liveSessions.map((session) => {
    const messages = messagesBySessionId.get(session.sessionId) ?? [];
    return {
      sessionId: session.sessionId,
      isCurrent: session.isCurrent,
      tokenUsage: aggregateSessionTokenUsage(messages),
      ram: ramBySessionId.get(session.sessionId) ?? {
        mappingState: "unavailable",
        reason: "unavailable_no_pid",
        sampledAtIso: (/* @__PURE__ */ new Date()).toISOString()
      }
    };
  });
}
async function readSessionMessages(sessionApi, sessionId) {
  const calls = [
    () => sessionApi.messages({ sessionId }),
    () => sessionApi.messages({ id: sessionId }),
    () => sessionApi.messages(sessionId)
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
function normalizeMessageArray(raw) {
  if (Array.isArray(raw)) {
    return raw;
  }
  if (!raw || typeof raw !== "object") {
    return [];
  }
  const candidate = raw;
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
async function resolveProcessMappingCandidate(sessionApi, session) {
  const fallback = {
    sessionId: session.sessionId
  };
  const calls = [
    () => sessionApi.get({ sessionId: session.sessionId }),
    () => sessionApi.get({ id: session.sessionId }),
    () => sessionApi.get(session.sessionId)
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
function extractMappingFromSessionDetail(detail, sessionId) {
  const records = flattenRecords(detail);
  let pid;
  let startTimeIso;
  for (const record of records) {
    pid = pid ?? firstFiniteInt(record, ["pid", "processPid", "process_pid"]);
    startTimeIso = startTimeIso ?? firstString(record, [
      "startTimeIso",
      "startedAtIso",
      "start_time_iso",
      "startTime",
      "startedAt"
    ]);
  }
  return {
    sessionId,
    pid,
    startTimeIso
  };
}
function flattenRecords(raw) {
  if (!raw || typeof raw !== "object") {
    return [];
  }
  const queue = [raw];
  const output = [];
  while (queue.length > 0) {
    const current = queue.shift();
    if (!current || typeof current !== "object") {
      continue;
    }
    const record = current;
    output.push(record);
    for (const value of Object.values(record)) {
      if (value && typeof value === "object") {
        queue.push(value);
      }
    }
  }
  return output;
}
function firstFiniteInt(record, keys) {
  for (const key of keys) {
    const value = record[key];
    if (typeof value === "number" && Number.isFinite(value) && Number.isInteger(value) && value > 0) {
      return value;
    }
  }
  return void 0;
}
function firstString(record, keys) {
  for (const key of keys) {
    const value = record[key];
    if (typeof value === "string" && value.length > 0) {
      return value;
    }
  }
  return void 0;
}
function summarizeRows(rows) {
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
    exactRamTotalBytes
  };
}
function sortRowsCurrentFirst(rows) {
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
function toSidebarRowView(row) {
  return {
    sessionId: row.sessionId,
    isCurrent: row.isCurrent,
    tokensTotal: row.tokenUsage.totalTokens,
    ramLabel: ramStateToLabel(row.ram)
  };
}
function ramStateToLabel(ram) {
  if (ram.mappingState === "exact") {
    return formatBytes(ram.bytes);
  }
  return `unavailable (${ram.reason ?? FALLBACK_UNAVAILABLE_LABEL})`;
}
function disabledItems(disabled) {
  return [
    {
      id: "disabled.reason",
      label: "Disabled",
      value: disabled.reason
    },
    {
      id: "disabled.message",
      label: "Reason",
      value: disabled.message
    }
  ];
}
function formatBytes(bytes) {
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

// src/index.ts
async function opencodeSessionMemorySidebarPlugin(context = {}) {
  const sessionApi = resolveSessionApi(context);
  const platform = resolvePlatform(context);
  const capabilityProbeInput = {
    platform,
    sidebarHook: () => [],
    sessionApi
  };
  const snapshot = () => buildSessionMemorySnapshot({
    capabilityProbeInput,
    sessionApi
  });
  return {
    sidebar: [
      createSessionMemorySidebarDefinition({
        id: "session-memory",
        snapshot
      })
    ]
  };
}
var index_default = opencodeSessionMemorySidebarPlugin;
function resolveSessionApi(context) {
  const fallback = createNoopSessionApi();
  const candidate = asRecord(context);
  if (!candidate) {
    return fallback;
  }
  const client = asRecord(candidate.client);
  const roots = [
    candidate.session,
    candidate.sessions,
    client?.session,
    asRecord(client?.api)?.session,
    asRecord(client?.experimental)?.session,
    asRecord(candidate.api)?.session,
    asRecord(candidate.experimental)?.session
  ];
  for (const root of roots) {
    const sessionApi = asSessionApiLike(root);
    if (sessionApi) {
      return sessionApi;
    }
  }
  return fallback;
}
function resolvePlatform(context) {
  const candidate = asRecord(context);
  if (!candidate) {
    return process.platform;
  }
  if (typeof candidate.platform === "string" && candidate.platform.length > 0) {
    return candidate.platform;
  }
  const system = asRecord(candidate.system);
  if (typeof system?.platform === "string" && system.platform.length > 0) {
    return system.platform;
  }
  return process.platform;
}
function createNoopSessionApi() {
  return {
    list: async () => [],
    get: async () => void 0,
    messages: async () => []
  };
}
function asSessionApiLike(value) {
  const record = asRecord(value);
  if (!record) {
    return null;
  }
  if (typeof record.list === "function" && typeof record.get === "function" && typeof record.messages === "function") {
    return {
      list: record.list,
      get: record.get,
      messages: record.messages
    };
  }
  return null;
}
function asRecord(value) {
  if (!value || typeof value !== "object") {
    return null;
  }
  return value;
}
export {
  DISABLED_REASON_MESSAGES,
  EMPTY_TOKEN_USAGE,
  FALLBACK_UNAVAILABLE_LABEL,
  SIDEBAR_PANEL_TITLE,
  SIDEBAR_POLL_INTERVAL_MS,
  aggregateSessionTokenUsage,
  buildSessionMemorySnapshot,
  buildSidebarItems,
  buildSidebarPanelModel,
  collectSessionMemoryRows,
  createRamState,
  createSessionMemorySidebarDefinition,
  index_default as default,
  discoverLiveSessions,
  hasUsableSessionApi,
  hasUsableSidebarHook,
  opencodeSessionMemorySidebarPlugin,
  probeRuntimeCapabilities
};

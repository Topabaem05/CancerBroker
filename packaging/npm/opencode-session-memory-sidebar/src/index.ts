import { createOpencodeClient } from "@opencode-ai/sdk";

import type { RuntimeCapabilityProbeInput } from "./capabilities";
import { summarizeVisibleSessions, type SessionApiLike } from "./live-sessions";
import { collectOpencodeHelperSummary, type OpencodeHelperSummary } from "./opencode-helpers";
import { buildSessionMemorySnapshot, buildSidebarItems, buildSidebarPanelModel } from "./sidebar";

const DEFAULT_SERVER_URL = process.env.OPENCODE_SERVER_URL || "http://localhost:4096";

export interface SessionMemoryToolContext {
  readonly sessionID?: string;
  readonly directory?: string;
}

export interface CreateSessionMemoryToolOptions {
  readonly platform?: string;
  readonly sessionApi?: SessionApiLike;
  readonly serverUrl?: string;
  readonly helperSummary?: OpencodeHelperSummary;
  readonly helperSummaryLoader?: () => Promise<OpencodeHelperSummary>;
}

export function createSessionMemoryTool(options: CreateSessionMemoryToolOptions = {}) {
  return {
    description:
      "Summarize live OpenCode session memory, token usage, and RAM attribution for the current session set.",
    args: {},
    execute: async (_args: Record<string, never>, context: SessionMemoryToolContext): Promise<string> => {
      const sessionApi = options.sessionApi ?? createRuntimeSessionApi(context, options.serverUrl);
      const capabilityProbeInput: RuntimeCapabilityProbeInput = {
        platform: options.platform ?? process.platform,
        sessionApi,
      };
      const snapshot = await buildSessionMemorySnapshot({
        capabilityProbeInput,
        sessionApi,
        currentSessionId: context.sessionID,
      });
      const visibleSessions = await summarizeVisibleSessions({
        sessionApi,
        currentSessionId: context.sessionID,
      });
      const helperSummary =
        options.helperSummary ?? (options.helperSummaryLoader ? await options.helperSummaryLoader() : await collectOpencodeHelperSummary());

      return formatSessionMemoryReport(buildSidebarPanelModel(snapshot), {
        currentDirectory: context.directory,
        storedSessionCount: visibleSessions.storedSessionCount,
        helperSummary,
      });
    },
  };
}

const sessionMemoryTool = createSessionMemoryTool();

export default sessionMemoryTool;

function createRuntimeSessionApi(context: SessionMemoryToolContext, serverUrl = DEFAULT_SERVER_URL): SessionApiLike {
  const client = createOpencodeClient({
    baseUrl: serverUrl,
    directory: context.directory,
  });

  return {
    list: (input?: unknown) => client.session.list({ query: input as Record<string, unknown> | undefined }),
    get: (input?: unknown) => {
      if (!input || typeof input !== "object") {
        return client.session.status();
      }

      const record = input as Record<string, unknown>;
      const id =
        (typeof record.id === "string" && record.id) ||
        (typeof record.sessionId === "string" && record.sessionId) ||
        (typeof record.session_id === "string" && record.session_id) ||
        undefined;

      return id ? client.session.get({ path: { id } }) : client.session.status();
    },
    messages: (input?: unknown) => {
      const id = resolveSessionId(input);
      if (!id) {
        return Promise.resolve([]);
      }

      return client.session.messages({ path: { id } });
    },
  };
}

function resolveSessionId(input: unknown): string | undefined {
  if (typeof input === "string" && input.length > 0) {
    return input;
  }

  if (!input || typeof input !== "object") {
    return undefined;
  }

  const record = input as Record<string, unknown>;
  if (typeof record.sessionId === "string" && record.sessionId.length > 0) {
    return record.sessionId;
  }
  if (typeof record.id === "string" && record.id.length > 0) {
    return record.id;
  }
  if (typeof record.session_id === "string" && record.session_id.length > 0) {
    return record.session_id;
  }

  return undefined;
}

function formatSessionMemoryReport(
  model: ReturnType<typeof buildSidebarPanelModel>,
  options: {
    readonly currentDirectory?: string;
    readonly storedSessionCount: number;
    readonly helperSummary: OpencodeHelperSummary;
  },
): string {
  const items = buildSidebarItems(model);
  const lines = ["# Session Memory"];

  if (model.capability.state === "disabled") {
    lines.push(`Capability: disabled (${model.capability.reason})`);
    lines.push(model.capability.message);
    return lines.join("\n");
  }

  lines.push("Summary:");
  for (const item of items.filter((item) => item.id.startsWith("summary."))) {
    lines.push(`- ${item.label}: ${item.value ?? ""}`.trim());
  }
  lines.push(`- Stored: ${options.storedSessionCount}`);
  lines.push(`- Opencode Helpers: ${options.helperSummary.activeCount}`);
  lines.push(`- Helper RAM: ${formatBytes(options.helperSummary.activeTotalBytes)}`);
  lines.push(`- Helper Cleanup: killed ${options.helperSummary.cleanupKilledCount}, skipped ${options.helperSummary.cleanupSkippedCount}`);

  if (model.current) {
    lines.push(`Current: ${model.current.sessionId} | tokens ${model.current.tokensTotal} | RAM ${model.current.ramLabel}`);
  }

  if (model.others.length > 0) {
    lines.push("Other live sessions:");
    for (const row of model.others) {
      lines.push(`- ${row.sessionId} | tokens ${row.tokensTotal} | RAM ${row.ramLabel}`);
    }
  }

  if (options.helperSummary.activeRows.length > 0) {
    lines.push("");
    lines.push("Opencode-owned helper processes:");
    for (const row of options.helperSummary.activeRows) {
      lines.push(`- ${row.label} | pid ${row.pid} | RAM ${formatBytes(row.rssBytes)} | age ${formatElapsedSeconds(row.elapsedSeconds)}`);
    }
  }

  if (model.summary.liveSessionCount === 0) {
    lines.push("");
    lines.push("Notes:");
    if (options.currentDirectory) {
      lines.push(`- Scope directory: ${options.currentDirectory}`);
    }
    lines.push("- This tool reports OpenCode sessions for the current project scope, plus Opencode-owned helper processes. It does not report unrelated local processes outside Opencode ownership.");
    if (options.helperSummary.activeCount > 0) {
      lines.push("- Opencode-owned helper processes are active, but they are not a substitute for live session records in the current project scope.");
    }
    if (options.storedSessionCount > 0) {
      lines.push("- Stored sessions exist, but none are currently live in this project scope. Reopen the session with `opencode -c` or `opencode -s <session-id>`.");
    } else {
      lines.push("- No stored sessions were found for the current project scope yet. Start or continue an OpenCode session in this directory first.");
    }
  }

  return lines.join("\n");
}

function formatElapsedSeconds(seconds: number): string {
  const hours = Math.floor(seconds / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  const remainingSeconds = seconds % 60;

  if (hours > 0) {
    return `${hours}h ${minutes}m ${remainingSeconds}s`;
  }
  if (minutes > 0) {
    return `${minutes}m ${remainingSeconds}s`;
  }
  return `${remainingSeconds}s`;
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

export * from "./capabilities";
export * from "./live-sessions";
export * from "./opencode-helpers";
export * from "./sidebar";
export * from "./token-usage";
export * from "./types";

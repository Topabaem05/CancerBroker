import { createOpencodeClient } from "@opencode-ai/sdk";

import type { RuntimeCapabilityProbeInput } from "./capabilities";
import type { SessionApiLike } from "./live-sessions";
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

      return formatSessionMemoryReport(buildSidebarPanelModel(snapshot));
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

function formatSessionMemoryReport(model: ReturnType<typeof buildSidebarPanelModel>): string {
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

  if (model.current) {
    lines.push(`Current: ${model.current.sessionId} | tokens ${model.current.tokensTotal} | RAM ${model.current.ramLabel}`);
  }

  if (model.others.length > 0) {
    lines.push("Other live sessions:");
    for (const row of model.others) {
      lines.push(`- ${row.sessionId} | tokens ${row.tokensTotal} | RAM ${row.ramLabel}`);
    }
  }

  return lines.join("\n");
}

export * from "./capabilities";
export * from "./live-sessions";
export * from "./sidebar";
export * from "./token-usage";
export * from "./types";

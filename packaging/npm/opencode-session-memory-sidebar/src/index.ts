import type { RuntimeCapabilityProbeInput } from "./capabilities";
import type { SessionApiLike } from "./live-sessions";
import {
  buildSessionMemorySnapshot,
  buildSidebarItems,
  buildSidebarPanelModel,
} from "./sidebar";

export async function opencodeSessionMemorySidebarPlugin(
  context: unknown = {},
): Promise<Record<string, unknown>> {
  const sessionApi = resolveSessionApi(context);
  const platform = resolvePlatform(context);
  const capabilityProbeInput: RuntimeCapabilityProbeInput = {
    platform,
    sidebarHook: () => [],
    sessionApi,
  };

  const snapshot = (currentSessionId?: string) =>
    buildSessionMemorySnapshot({
      capabilityProbeInput,
      sessionApi,
      currentSessionId,
    });

  return {
    tool: {
      session_memory: {
        description:
          "Summarize live OpenCode session memory, token usage, and RAM attribution for the current session set.",
        args: {},
        execute: async (_args: Record<string, never>, toolContext: { sessionID?: string }) => {
          const model = buildSidebarPanelModel(await snapshot(toolContext?.sessionID));
          return formatSessionMemoryReport(model);
        },
      },
    },
  };
}

export default opencodeSessionMemorySidebarPlugin;

function resolveSessionApi(context: unknown): SessionApiLike {
  const fallback = createNoopSessionApi();
  const candidate = asRecord(context);
  if (!candidate) {
    return fallback;
  }

  const client = asRecord(candidate.client);
  const roots: unknown[] = [
    candidate.session,
    candidate.sessions,
    client?.session,
    asRecord(client?.api)?.session,
    asRecord(client?.experimental)?.session,
    asRecord(candidate.api)?.session,
    asRecord(candidate.experimental)?.session,
  ];

  for (const root of roots) {
    const sessionApi = asSessionApiLike(root);
    if (sessionApi) {
      return sessionApi;
    }
  }

  return fallback;
}

function resolvePlatform(context: unknown): string {
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

function createNoopSessionApi(): SessionApiLike {
  return {
    list: async () => [],
    get: async () => undefined,
    messages: async () => [],
  };
}

function asSessionApiLike(value: unknown): SessionApiLike | null {
  const record = asRecord(value);
  if (!record) {
    return null;
  }

  if (
    typeof record.list === "function" &&
    typeof record.get === "function" &&
    typeof record.messages === "function"
  ) {
    return {
      list: record.list as SessionApiLike["list"],
      get: record.get as SessionApiLike["get"],
      messages: record.messages as SessionApiLike["messages"],
    };
  }

  return null;
}

function asRecord(value: unknown): Record<string, unknown> | null {
  if (!value || typeof value !== "object") {
    return null;
  }

  return value as Record<string, unknown>;
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

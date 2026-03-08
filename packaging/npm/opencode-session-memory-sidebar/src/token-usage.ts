import type { SessionTokenUsage } from "./types";

export const EMPTY_TOKEN_USAGE: SessionTokenUsage = {
  inputTokens: 0,
  outputTokens: 0,
  reasoningTokens: 0,
  cacheReadTokens: 0,
  totalTokens: 0,
};

export function aggregateSessionTokenUsage(messages: readonly unknown[]): SessionTokenUsage {
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
    totalTokens,
  };
}

function isAssistantMessage(message: unknown): boolean {
  if (!message || typeof message !== "object") {
    return false;
  }

  const candidate = message as Record<string, unknown>;
  const role = asString(candidate.role) ?? asString(candidate.type) ?? "";
  return role.toLowerCase() === "assistant";
}

function extractUsageBuckets(message: unknown): Omit<SessionTokenUsage, "totalTokens"> {
  const usageScopes = gatherUsageScopes(message);

  const inputTokens = readFirstNumber(usageScopes, [
    ["inputTokens"],
    ["input_tokens"],
    ["input"],
    ["promptTokens"],
    ["prompt_tokens"],
    ["prompt"],
  ]);

  const outputTokens = readFirstNumber(usageScopes, [
    ["outputTokens"],
    ["output_tokens"],
    ["output"],
    ["completionTokens"],
    ["completion_tokens"],
    ["completion"],
  ]);

  const reasoningTokens = readFirstNumber(usageScopes, [
    ["reasoningTokens"],
    ["reasoning_tokens"],
    ["reasoning"],
  ]);

  const cacheReadTokens = readFirstNumber(usageScopes, [
    ["cacheReadTokens"],
    ["cache_read_tokens"],
    ["cacheReadInputTokens"],
    ["cache_read_input_tokens"],
    ["cacheRead"],
    ["cache", "read"],
  ]);

  return {
    inputTokens,
    outputTokens,
    reasoningTokens,
    cacheReadTokens,
  };
}

function gatherUsageScopes(message: unknown): Record<string, unknown>[] {
  if (!message || typeof message !== "object") {
    return [];
  }

  const candidate = message as Record<string, unknown>;
  const scopes: Record<string, unknown>[] = [];

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
    if (isRecord((candidate.metrics as Record<string, unknown>).tokens)) {
      scopes.push((candidate.metrics as Record<string, unknown>).tokens as Record<string, unknown>);
    }
  }

  return scopes;
}

function readFirstNumber(scopes: readonly Record<string, unknown>[], paths: readonly string[][]): number {
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

function readPath(record: Record<string, unknown>, path: readonly string[]): unknown {
  let current: unknown = record;
  for (const key of path) {
    if (!current || typeof current !== "object") {
      return undefined;
    }
    current = (current as Record<string, unknown>)[key];
  }
  return current;
}

function isFiniteNonNegativeNumber(value: unknown): value is number {
  return typeof value === "number" && Number.isFinite(value) && value >= 0;
}

function asString(value: unknown): string | undefined {
  return typeof value === "string" && value.length > 0 ? value : undefined;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return !!value && typeof value === "object";
}

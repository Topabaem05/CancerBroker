import { expect, test } from "bun:test";

import plugin, { opencodeSessionMemorySidebarPlugin } from "./index";

test("exports the plugin entrypoint as the default export", () => {
  expect(plugin).toBe(opencodeSessionMemorySidebarPlugin);
  expect(plugin).toBeInstanceOf(Function);
});

test("registers a supported session_memory tool", async () => {
  const result = await plugin({
    platform: "darwin",
    session: {
      list: async () => [{ id: "session-1", status: "running" }],
      get: async () => ({ id: "session-1" }),
      messages: async () => [],
    },
  });

  const pluginRecord = asRecord(result);
  const tool = asRecord(pluginRecord.tool);
  const sessionMemory = asRecord(tool.session_memory);

  expect(sessionMemory.description).toBeString();
  expect(typeof sessionMemory.execute).toBe("function");

  const output = await (sessionMemory.execute as (
    args: Record<string, never>,
    context: { sessionID?: string },
  ) => Promise<string>)({}, { sessionID: "session-1" });

  expect(output).toContain("# Session Memory");
  expect(output).toContain("Summary:");
  expect(output).toContain("Current: session-1");
});

function asRecord(value: unknown): Record<string, unknown> {
  if (!value || typeof value !== "object") {
    throw new Error("Expected an object record");
  }

  return value as Record<string, unknown>;
}

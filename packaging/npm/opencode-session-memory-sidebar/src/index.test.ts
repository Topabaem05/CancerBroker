import { expect, test } from "bun:test";

import tool, { createSessionMemoryTool } from "./index";

test("exports the session_memory tool as the default export", () => {
  const created = createSessionMemoryTool();
  expect(tool.description).toBe(created.description);
  expect(tool.args).toEqual(created.args);
  expect(tool.description).toBeString();
  expect(typeof tool.execute).toBe("function");
});

test("builds a supported session_memory tool report", async () => {
  const sessionMemoryTool = createSessionMemoryTool({
    platform: "darwin",
    sessionApi: {
      list: async () => [{ id: "session-1", status: "running" }],
      get: async () => ({ id: "session-1" }),
      messages: async () => [],
    },
  });

  const output = await (sessionMemoryTool.execute as (
    args: Record<string, never>,
    context: { sessionID?: string },
  ) => Promise<string>)({}, { sessionID: "session-1" });

  expect(output).toContain("# Session Memory");
  expect(output).toContain("Summary:");
  expect(output).toContain("Stored: 1");
  expect(output).toContain("Current: session-1");
});

test("explains project-scoped empty results when no live sessions exist", async () => {
  const sessionMemoryTool = createSessionMemoryTool({
    platform: "darwin",
    sessionApi: {
      list: async () => [{ id: "session-9", status: "completed" }],
      get: async () => ({ id: "session-9" }),
      messages: async () => [],
    },
  });

  const output = await sessionMemoryTool.execute({}, {
    sessionID: "session-9",
    directory: "/Users/guribbong/code/testest",
  });

  expect(output).toContain("Stored: 1");
  expect(output).toContain("Live: 0");
  expect(output).toContain("Scope directory: /Users/guribbong/code/testest");
  expect(output).toContain("not unrelated local processes like biome or tsserver");
  expect(output).toContain("Stored sessions exist, but none are currently live");
});

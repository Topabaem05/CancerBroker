import { expect, test } from "bun:test";

import plugin, { opencodeSessionMemorySidebarPlugin } from "./index";

test("exports the plugin entrypoint as the default export", () => {
  expect(plugin).toBe(opencodeSessionMemorySidebarPlugin);
  expect(plugin).toBeInstanceOf(Function);
});

test("builds a sidebar definition with runtime items", async () => {
  const result = await plugin({
    platform: "darwin",
    session: {
      list: async () => [{ id: "session-1", status: "running" }],
      get: async () => ({ id: "session-1" }),
      messages: async () => [],
    },
  });

  const pluginRecord = asRecord(result);
  const sidebar = pluginRecord.sidebar;

  expect(Array.isArray(sidebar)).toBe(true);
  if (!Array.isArray(sidebar)) {
    throw new Error("Expected sidebar array");
  }
  expect(sidebar).toHaveLength(1);

  const definition = asRecord(sidebar[0]);
  expect(definition.id).toBe("session-memory");
  expect(definition.title).toBe("Session Memory");
  expect(typeof definition.items).toBe("function");

  const items = await (definition.items as () => Promise<unknown[]>)();
  expect(Array.isArray(items)).toBe(true);
  expect(items.some(isLiveSummaryItem)).toBe(true);
});

function asRecord(value: unknown): Record<string, unknown> {
  if (!value || typeof value !== "object") {
    throw new Error("Expected an object record");
  }

  return value as Record<string, unknown>;
}

function isLiveSummaryItem(value: unknown): boolean {
  if (!value || typeof value !== "object") {
    return false;
  }

  const candidate = value as Record<string, unknown>;
  return candidate.id === "summary.live" && candidate.label === "Live";
}

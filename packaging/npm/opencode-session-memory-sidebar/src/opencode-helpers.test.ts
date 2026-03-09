import { expect, test } from "bun:test";

import { collectOpencodeHelperSummary, parseElapsedSeconds, parseProcessRow } from "./opencode-helpers";

test("parses ps rows with pid parent pgid and rss", () => {
  const row = parseProcessRow(
    "21658 25671 21658 S+ 04:53:32 496480 opencode opencode",
  );

  expect(row).not.toBeNull();
  expect(row?.pid).toBe(21658);
  expect(row?.ppid).toBe(25671);
  expect(row?.pgid).toBe(21658);
  expect(row?.rssBytes).toBe(496480 * 1024);
});

test("parses elapsed time variants", () => {
  expect(parseElapsedSeconds("59")).toBe(0);
  expect(parseElapsedSeconds("04:39")).toBe(279);
  expect(parseElapsedSeconds("01:02:03")).toBe(3723);
  expect(parseElapsedSeconds("01-00:00:05")).toBe(86405);
});

test("summarizes Opencode-owned helpers and cleans stale duplicates", async () => {
  const killed: number[] = [];
  const summary = await collectOpencodeHelperSummary({
    currentPid: 100,
    sampleProcesses: async () => [
      {
        pid: 100,
        ppid: 1,
        pgid: 100,
        state: "S+",
        elapsedSeconds: 120,
        rssBytes: 100 * 1024,
        command: "opencode",
        args: "opencode",
      },
      {
        pid: 201,
        ppid: 100,
        pgid: 100,
        state: "S+",
        elapsedSeconds: 90,
        rssBytes: 4 * 1024 * 1024,
        command: "node",
        args: "node /Users/guribbong/.npm-global/bin/biome lsp-proxy --stdio",
      },
      {
        pid: 202,
        ppid: 100,
        pgid: 100,
        state: "S+",
        elapsedSeconds: 5,
        rssBytes: 8 * 1024 * 1024,
        command: "node",
        args: "node /Users/guribbong/.npm-global/bin/biome lsp-proxy --stdio",
      },
      {
        pid: 203,
        ppid: 100,
        pgid: 100,
        state: "S+",
        elapsedSeconds: 20,
        rssBytes: 16 * 1024 * 1024,
        command: "node",
        args: "node /Users/guribbong/.npm-global/bin/typescript-language-server --stdio",
      },
    ],
    signalProcess: (pid) => {
      killed.push(pid);
    },
    isProcessAlive: () => false,
    staleThresholdSeconds: 30,
  });

  expect(summary.activeCount).toBe(3);
  expect(summary.activeTotalBytes).toBe((4 + 8 + 16) * 1024 * 1024);
  expect(summary.cleanupKilledCount).toBe(1);
  expect(summary.cleanupSkippedCount).toBe(0);
  expect(killed).toEqual([201]);
  expect(summary.activeRows[0]?.label).toBe("typescript-language-server");
});

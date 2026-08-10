import { expect, test } from "bun:test";
import { mkdir, mkdtemp, readFile, readdir, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";

test("a reloaded adapter does not duplicate transcript or live-context turns", async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), "omp-conversation-dedupe-"));
  const cwd = path.join(root, "example");
  await mkdir(cwd, { recursive: true });
  await writeFile(path.join(cwd, ".solarisael-room.json"), `${JSON.stringify({
    version: 1,
    room: "example",
    trueName: "Example Room",
    operator: "Example Operator",
  })}\n`, "utf8");
  await writeFile(path.join(root, "shared_current_state.md"), "# Shared state\n", "utf8");

  try {
    const first = await import("../solarisael-house-proof/conversation-log.ts?dedupe-instance=1");
    const second = await import("../solarisael-house-proof/conversation-log.ts?dedupe-instance=2");
    const messages = [{ role: "user", id: "stable-turn", content: "One durable turn." }];
    const ctx = { cwd, sessionID: "dedupe-session" };

    await first.logUnseenConversationTurns(ctx, messages, "first-instance");
    await second.logUnseenConversationTurns(ctx, messages, "second-instance");

    const live = JSON.parse(await readFile(path.join(cwd, "current_session_context.json"), "utf8"));
    expect(live.recentTurns).toHaveLength(1);
    expect(live.recentTurns[0]).toMatchObject({ role: "user", text: "One durable turn." });

    const transcriptName = (await readdir(cwd)).find((name) => /^conversation_log_.*\.md$/.test(name));
    expect(transcriptName).toBeDefined();
    const transcript = await readFile(path.join(cwd, transcriptName!), "utf8");
    expect(transcript.match(/One durable turn\./g)).toHaveLength(1);
    expect(transcript).toMatch(/## \d{2}:\d{2} — Example Operator/);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

// GIGA ingested zero events in every room from launch until 2026-07-25 because
// this harness supplies no message id, and every fixture in giga-worker.test.ts
// hardcodes `hasStableID: true`. The one value that fails in production was
// stubbed to pass in every test. These cases exercise the opposite assumption.

const RFC3339 = /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(\.\d+)?(Z|[+-]\d{2}:\d{2})$/;
const idOf = (turns: any[]) => turns.map((turn) => String(turn.messageID));

test("a turn with no harness id still gets a stable identity", async () => {
  const { conversationTurns } = await import("../solarisael-house-proof/conversation-log.ts?identity-derived");
  const [turn] = conversationTurns([{ role: "user", content: "No id anywhere." }]);

  expect(turn.hasVisibleID).toBe(false);
  expect(turn.hasStableID).toBe(true);
  expect(String(turn.messageID)).toMatch(/^omp-derived:user:0:[0-9a-f]{32}$/);
});

test("the derived identity is identical across repeated reads of the same turn", async () => {
  const { conversationTurns } = await import("../solarisael-house-proof/conversation-log.ts?identity-stable");
  const messages = [{ role: "assistant", content: "Same turn, read twice." }];

  expect(idOf(conversationTurns(messages))).toEqual(idOf(conversationTurns(messages)));
});

test("two identical turns in one window get different source ids", async () => {
  const { conversationTurns } = await import("../solarisael-house-proof/conversation-log.ts?identity-unique");
  const ids = idOf(conversationTurns([
    { role: "user", content: "ok" },
    { role: "assistant", content: "ok" },
    { role: "user", content: "ok" },
  ]));

  // buildGigaConversationWindow rejects the whole event when source ids repeat,
  // so duplicate text must not collapse to one identity.
  expect(new Set(ids).size).toBe(ids.length);
});

test("a harness-supplied id wins over derivation", async () => {
  const { conversationTurns } = await import("../solarisael-house-proof/conversation-log.ts?identity-harness");
  const [turn] = conversationTurns([{ role: "user", id: "harness-uuid-7", content: "Real id present." }]);

  expect(turn.hasVisibleID).toBe(true);
  expect(turn.messageID).toBe("harness-uuid-7");
});

test("a turn with no harness timestamp still carries an RFC3339 timestamp", async () => {
  const { conversationTurns } = await import("../solarisael-house-proof/conversation-log.ts?stamp-derived");
  const [turn] = conversationTurns([{ role: "user", content: "No timestamp either." }]);

  // GIGA hard-rejects a window whose turns fail isRfc3339, so absence is fatal.
  expect(turn.sourceTimestamp).toMatch(RFC3339);
});

test("a harness-supplied timestamp is never overwritten", async () => {
  const { conversationTurns } = await import("../solarisael-house-proof/conversation-log.ts?stamp-harness");
  const [turn] = conversationTurns([
    { role: "user", content: "Stamped.", timestamp: "2026-07-24T12:00:00.000Z" },
  ]);

  expect(turn.sourceTimestamp).toBe("2026-07-24T12:00:00.000Z");
});

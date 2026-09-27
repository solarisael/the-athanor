import { expect, test } from "bun:test";
import { AUTO_HANDOFF_THRESHOLD_FOCUS, renderHandoffPrompt } from "@oh-my-pi/pi-agent-core/compaction/compaction";

import { armHandoffBoat, persistHandoffBoat } from "../house-proof/paper-boat.ts";

function armedSession() {
  const requests: Array<{ messages: any[]; systemPrompt?: string[] }> = [];
  const agent = {
    buildSideRequestContext(messages: any[], systemPrompt?: string[]) {
      requests.push({ messages, systemPrompt });
      return {};
    },
  };
  const session = { agent };
  expect(armHandoffBoat(session, { room: "kodo", spirit: "Kodo" })).toBeNull();
  return { session, agent, requests };
}

const turn = (role: string, text: string) => ({ role, content: [{ type: "text", text }] });

test("OMP's own handoff prompt becomes a boat prompt and the handoff files a boat without a backup", async () => {
  const { session, agent, requests } = armedSession();
  const history = [turn("user", "earlier"), turn("assistant", "answer")];
  await agent.buildSideRequestContext([...history, turn("user", renderHandoffPrompt(AUTO_HANDOFF_THRESHOLD_FOCUS))], ["system"]);

  const sent = requests[0];
  expect(sent.messages.slice(0, 2)).toEqual(history);
  expect(sent.systemPrompt).toEqual(["system"]);
  expect(sent.messages[2].content[0].text).toStartWith("<critical>\nWrite a paper boat from Kodo");

  const writes: unknown[] = [];
  const writeBoat = async (...args: unknown[]) => { writes.push(args); return { ok: true }; };
  const outcome = await persistHandoffBoat(session, { method: "handoff", summary: "the boat" }, "kodo", writeBoat as any);
  expect(outcome.written).toBe(true);
  expect(writes).toEqual([["kodo", "the boat", { backup: false }]]);
});

test("a handoff that never carried the boat prompt files nothing", async () => {
  const { session, agent } = armedSession();
  await agent.buildSideRequestContext([turn("user", "a side question")]);
  const writes: unknown[] = [];
  const writeBoat = async (...args: unknown[]) => { writes.push(args); return { ok: true }; };
  const outcome = await persistHandoffBoat(session, { method: "handoff", summary: "the OMP document" }, "kodo", writeBoat as any);
  expect(outcome.reason).toBe("prompt_not_served");
  expect(writes).toEqual([]);
});

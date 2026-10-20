// The reconnect contract: a dead Host arms a growing retry clock, a Host that
// comes back is asked again without a reload, and nothing pending is re-sent.
// The clock is observed where it acts — the timing of /live/health requests.
// Real timers, so this file waits for the first two retry delays (2 s, 4 s).
import { test, expect } from "bun:test";
import { queryHealthHost, onHostRecovered, roomState } from "./health.js";
import { initChat, syncChatPanel, say, querySnapshot, chatMessages, chatBlockReason, chatState } from "./chat.js";

const ROOM = { kind: "direct", id: "kodo" };
let hostUp = false;
const healthAsks = [];
const says = [];
const ring = [];
let loseReply = false;
let invalidReply = false;
let refuseNextSay = 0;

globalThis.fetch = async (path, init) => {
  if (path === "/live/health") healthAsks.push(Date.now());
  if (!hostUp) {
    return Response.json({ error: "Host request failed: connect refused", hop: "host_unreachable" }, { status: 502 });
  }
  if (path === "/live/health") return Response.json({ status: "ok", websocket_path: "/room/kodo/athanor/v1/ws", sequence: 1, version: 1, projection_id: "p", state_hash: "h" });
  if (path === "/live/room/state") return Response.json({ room: "kodo", operator: "Sol", presences: [] });
  if (path === "/live/chat/snapshot") return Response.json({ room: "kodo", messages: ring });
  if (path === "/live/chat/say") {
    const body = JSON.parse(init.body);
    says.push(body.sayId);

    if (refuseNextSay) {
      const status = refuseNextSay;
      refuseNextSay = 0;
      return Response.json({ error: "Retry refused before delivery" }, { status });
    }
    const message = { sequence: ring.length + 1, author: "operator", authorName: "Sol", text: body.text, at: new Date().toISOString(), turnId: body.sayId };
    ring.push(message);

    if (invalidReply) {
      invalidReply = false;
      return Response.json({ accepted: true });
    }

    if (loseReply) {
      loseReply = false;
      return Response.json({ error: "Response lost after acceptance" }, { status: 502 });
    }

    return Response.json({ room: "kodo", accepted: true, repeated: false, message });
  }
  return Response.json({ error: "unknown" }, { status: 404 });
};

const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
const gap = index => healthAsks[index] - healthAsks[index - 1];

test("a dead Host is asked again with a growing retry delay", async () => {
  await queryHealthHost();
  expect(healthAsks.length).toBe(1);
  await sleep(2300);
  expect(healthAsks.length).toBe(2);
  expect(gap(1)).toBeGreaterThanOrEqual(1900);
  expect(gap(1)).toBeLessThan(3000);
});

test("a Host that comes back fires recovery once, and chat retries by hand with the same sayId", async () => {
  let recovered = 0;
  let accepted = 0;
  const onAccepted = () => { accepted += 1; };
  onHostRecovered(() => { recovered += 1; });
  initChat({ requestRender: () => {} });

  hostUp = true;
  await sleep(4300);
  expect(healthAsks.length).toBe(3);
  expect(gap(2)).toBeGreaterThanOrEqual(3900);
  expect(recovered).toBe(1);
  expect(roomState().room).toBe("kodo");

  syncChatPanel(ROOM, "live");
  await sleep(50);
  expect(chatBlockReason(ROOM)).toBeNull();

  hostUp = false;
  expect(await say("hewwo", onAccepted)).toBe(false);
  const line = chatMessages().find(message => message.text === "hewwo");
  expect(line.unconfirmed).toBe(true);
  expect(says).toEqual([]);

  hostUp = true;
  await sleep(2300);
  expect(recovered).toBe(2);
  expect(chatState().reason).toBeNull();
  expect(chatMessages().find(message => message.text === "hewwo").unconfirmed).toBe(true);

  expect(await say("hewwo", onAccepted)).toBe(true);
  expect(says.length).toBe(1);
  expect(says[0]).toBe(line.turnId);
  expect(accepted).toBe(1);
  const settled = chatMessages().filter(message => message.text === "hewwo");
  expect(settled.length).toBe(1);
  expect(settled[0].turnId).toBe(says[0]);
  expect(settled[0].pending).toBe(false);
}, 15000);

test("snapshot acceptance retires the pending send before an intentional identical message", async () => {
  let accepted = 0;
  const onAccepted = () => { accepted += 1; };
  const writesBefore = says.length;
  loseReply = true;

  expect(await say("a deliberate repeat", onAccepted)).toBe(false);
  expect(accepted).toBe(0);
  expect(chatMessages().find(message => message.text === "a deliberate repeat").unconfirmed).toBe(true);

  await querySnapshot();
  expect(accepted).toBe(1);
  expect(says.length).toBe(writesBefore + 1);
  expect(chatMessages().filter(message => message.text === "a deliberate repeat")).toHaveLength(1);

  expect(await say("a deliberate repeat", onAccepted)).toBe(true);
  expect(accepted).toBe(2);
  expect(says[writesBefore + 1]).not.toBe(says[writesBefore]);
});

test("a refused retry preserves an earlier ambiguous send until snapshot acceptance", async () => {
  for (const status of [401, 403, 404, 429]) {
    const text = `ambiguous send followed by ${status}`;
    let accepted = 0;
    const onAccepted = () => { accepted += 1; };
    const writesBefore = says.length;
    invalidReply = true;

    expect(await say(text, onAccepted)).toBe(false);
    const turnId = says[writesBefore];
    refuseNextSay = status;

    expect(await say(text, onAccepted)).toBe(false);
    expect(says[writesBefore + 1]).toBe(turnId);
    expect(chatMessages().find(message => message.turnId === turnId)?.unconfirmed).toBe(true);
    expect(accepted).toBe(0);

    await querySnapshot();
    expect(accepted).toBe(1);
    expect(chatState().refusal).toBeNull();
    expect(chatMessages().filter(message => message.turnId === turnId)).toHaveLength(1);
    expect(says.length).toBe(writesBefore + 2);

    expect(await say(text, onAccepted)).toBe(true);
    expect(says[writesBefore + 2]).not.toBe(turnId);
    expect(accepted).toBe(2);
  }
});

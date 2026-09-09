// The reconnect contract: a dead Host arms a growing retry clock, a Host that
// comes back is asked again without a reload, and nothing pending is re-sent.
// Real timers, so this file waits for the first two retry delays (2 s, 4 s).
import { test, expect } from "bun:test";
import { queryHealthHost, reconnectState, onHostRecovered, roomState, healthSourceLine } from "./health.js";
import { initChat, syncChatPanel, say, chatMessages, chatBlockReason, chatState } from "./chat.js";

const ROOM = { kind: "direct", id: "kodo" };
let hostUp = false;
const says = [];
const ring = [];

globalThis.fetch = async (path, init) => {
  if (!hostUp) {
    return Response.json({ error: "Host request failed: connect refused", hop: "host_unreachable" }, { status: 502 });
  }
  if (path === "/live/health") return Response.json({ status: "ok", websocket_path: "/room/kodo/athanor/v1/ws", sequence: 1, version: 1, projection_id: "p", state_hash: "h" });
  if (path === "/live/room/state") return Response.json({ room: "kodo", operator: "Sol", presences: [] });
  if (path === "/live/chat/snapshot") return Response.json({ room: "kodo", messages: ring });
  if (path === "/live/chat/say") {
    const body = JSON.parse(init.body);
    says.push(body.sayId);
    ring.push({ sequence: ring.length + 1, author: "operator", authorName: "Sol", text: body.text, at: new Date().toISOString(), turnId: body.sayId });
    return Response.json({ room: "kodo", accepted: true });
  }
  return Response.json({ error: "unknown" }, { status: 404 });
};

const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));

test("a dead Host arms 2 s then 4 s retries, and the reason names the hop", async () => {
  await queryHealthHost();
  expect(reconnectState()).toEqual({ attempt: 1, delayMs: 2000, down: true });
  expect(healthSourceLine()).toBe("Host unreachable · Host not reachable · retry 1 in 2 s");
  await sleep(2200);
  expect(reconnectState()).toEqual({ attempt: 2, delayMs: 4000, down: true });
});

test("a Host that comes back fires recovery once, and chat retries by hand with the same sayId", async () => {
  let recovered = 0;
  onHostRecovered(() => { recovered += 1; });
  initChat({ requestRender: () => {} });

  hostUp = true;
  await sleep(4300);
  expect(recovered).toBe(1);
  expect(reconnectState()).toEqual({ attempt: 0, delayMs: null, down: false });
  expect(roomState().room).toBe("kodo");

  syncChatPanel(ROOM, "live");
  await sleep(50);
  expect(chatBlockReason(ROOM)).toBeNull();

  hostUp = false;
  expect(await say("hewwo")).toBe(false);
  const line = chatMessages().find(message => message.text === "hewwo");
  expect(line.undelivered).toBe(true);
  expect(chatBlockReason(ROOM)).toBe("Host unreachable · Host not reachable");
  expect(reconnectState().down).toBe(true);
  expect(says).toEqual([]);

  hostUp = true;
  await sleep(2300);
  expect(recovered).toBe(2);
  expect(chatState().reason).toBeNull();
  expect(chatMessages().find(message => message.text === "hewwo").undelivered).toBe(true);

  expect(await say("hewwo")).toBe(true);
  expect(says.length).toBe(1);
  const settled = chatMessages().filter(message => message.text === "hewwo");
  expect(settled.length).toBe(1);
  expect(settled[0].turnId).toBe(says[0]);
  expect(settled[0].pending).toBe(false);
}, 15000);

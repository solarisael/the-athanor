// The reconnect contract: a dead Host arms a growing retry clock, a Host that
// comes back is asked again without a reload, and nothing pending is re-sent.
// The clock is observed where it acts — the timing of /live/health requests.
// Real timers, so this file waits for the first two retry delays (2 s, 4 s).
import { test, expect } from "bun:test";
import { queryHealthHost, onHostRecovered, roomState, healthSourceLine } from "./health.js";
import { initChat, syncChatPanel, say, chatMessages, chatBlockReason, chatState } from "./chat.js";

const ROOM = { kind: "direct", id: "kodo" };
let hostUp = false;
const healthAsks = [];
const says = [];
const ring = [];

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
    ring.push({ sequence: ring.length + 1, author: "operator", authorName: "Sol", text: body.text, at: new Date().toISOString(), turnId: body.sayId });
    return Response.json({ room: "kodo", accepted: true });
  }
  return Response.json({ error: "unknown" }, { status: 404 });
};

const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
const gap = index => healthAsks[index] - healthAsks[index - 1];

test("a dead Host is asked again after 2 s, then 4 s, and the reason names the hop", async () => {
  await queryHealthHost();
  expect(healthAsks.length).toBe(1);
  expect(healthSourceLine()).toBe("Host unreachable · Host not reachable · retry 1 in 2 s");
  await sleep(2300);
  expect(healthAsks.length).toBe(2);
  expect(gap(1)).toBeGreaterThanOrEqual(1900);
  expect(gap(1)).toBeLessThan(3000);
  expect(healthSourceLine()).toBe("Host unreachable · Host not reachable · retry 2 in 4 s");
});

test("a Host that comes back fires recovery once, and chat retries by hand with the same sayId", async () => {
  let recovered = 0;
  onHostRecovered(() => { recovered += 1; });
  initChat({ requestRender: () => {} });

  hostUp = true;
  await sleep(4300);
  expect(healthAsks.length).toBe(3);
  expect(gap(2)).toBeGreaterThanOrEqual(3900);
  expect(recovered).toBe(1);
  expect(healthSourceLine()).toMatch(/^Host connected · kodo room health/);
  expect(roomState().room).toBe("kodo");

  syncChatPanel(ROOM, "live");
  await sleep(50);
  expect(chatBlockReason(ROOM)).toBeNull();

  hostUp = false;
  expect(await say("hewwo")).toBe(false);
  const line = chatMessages().find(message => message.text === "hewwo");
  expect(line.undelivered).toBe(true);
  expect(chatBlockReason(ROOM)).toBe("Host unreachable · Host not reachable");
  expect(healthSourceLine()).toBe("Host unreachable · Host not reachable · retry 1 in 2 s");
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

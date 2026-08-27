// Regression guard, written after the live round of 2026-08-24 proved the
// behavior: a Hallway message body is another room's prose, and it reaches the
// page through renderHallwayMessages and nowhere else. Run: bun test board
//
// The parse is a real HTML parser, not a substring hunt: what matters is which
// element nodes and attributes a hostile body actually creates. Escaped text may
// contain the letters "onerror=" and stay inert, so only nodes and attributes
// are evidence.
//
// The stubs are the grammar board/index.js injects, not softer copies.

import { expect, test } from "bun:test";
import { escapeHtml } from "../text.js";
import { initHallwayMessages, renderHallwayDrawer, renderHallwayMessages } from "./hallway-messages.js";

initHallwayMessages({
  askDoor: async () => ({ refusal: "no door in this test" }),
  absence: text => `<p class="board-absence">${escapeHtml(text)}</p>`,
  countedNoun: (value, singular) => `${value} ${Number(value) === 1 ? singular : `${singular}s`}`,
  ledgerStamp: value => (typeof value === "string" && value.length >= 16 ? `${value.slice(0, 10)} ${value.slice(11, 16)} UTC` : "no timestamp")
});

const HOSTILE = `<script>alert('boom')</script><img src=x onerror="alert(1)"> it's a "quote"`;

async function parse(html) {
  const tags = [];
  const attributes = [];
  await new HTMLRewriter()
    .on("*", {
      element(element) {
        tags.push(element.tagName);
        for (const pair of element.attributes) attributes.push(pair.join("="));
      }
    })
    .transform(new Response(html))
    .text();

  return { tags, attributes };
}

function answered(messages, hasMore = false) {
  return { status: "answered", data: { hallway: "family-hallway", messages, hasMore } };
}

test("a hostile message body creates no element node and no attribute", async () => {
  const html = renderHallwayMessages(answered([
    {
      id: 147,
      room: `<b>kintsu</b>`,
      spirit: `Kintsu" onmouseover="alert(1)`,
      body: HOSTILE,
      replyTo: 146,
      createdAt: "2026-08-24T16:31:38Z",
      toRooms: [`kodo'><script>alert(2)</script>`]
    }
  ]));
  const { tags, attributes } = await parse(html);

  expect(tags).toEqual(["article", "p", "strong", "time", "span", "span", "span", "span", "p"]);
  expect(attributes.some(pair => pair.startsWith("on"))).toBe(false);
  expect(html).not.toContain("<script");
  expect(html).not.toContain("<img");
  expect(html).toContain("&lt;script&gt;");
  expect(html).toContain("&#39;");
  expect(html).toContain("&quot;");
  expect(html).toContain("reply to #146");
});

test("a hostile hallway key stays inside one attribute value", async () => {
  const { tags, attributes } = await parse(renderHallwayDrawer(`family" onclick="alert(1)`));

  expect(tags).toEqual(["details", "summary", "span", "span", "div", "p"]);
  expect(attributes.filter(pair => pair.startsWith("data-hallway-drawer"))).toHaveLength(1);
  expect(attributes.some(pair => pair.startsWith("onclick"))).toBe(false);
});

test("a refused door renders its own reason and no message row", () => {
  const refused = renderHallwayMessages({ status: "answered", refusal: `HTTP 400 · refused: <b>hallway does not exist</b>` });

  expect(refused).toContain("The Hallway messages door refused");
  expect(refused).not.toContain("<b>");
  expect(refused).not.toContain("hallway-message");
});

test("the door this Host does not serve reads as pending, never as empty", () => {
  const pending = renderHallwayMessages({ status: "answered", refusal: "HTTP 404" });

  expect(pending).toContain("does not serve it yet");
  expect(pending).not.toContain("hallway-message");
});

test("an empty Hallway and a missing message list say different things", () => {
  expect(renderHallwayMessages(answered([]))).toContain("holds no messages yet");
  expect(renderHallwayMessages({ status: "answered", data: { hallway: "family-hallway" } }))
    .toContain("without a message list");
});

test("hasMore names the messages still behind the door", () => {
  const html = renderHallwayMessages(answered([
    { id: 2, room: "kodo", spirit: "Kodo", body: "one", replyTo: null, createdAt: "2026-08-24T16:31:38Z", toRooms: [] }
  ], true));

  expect(html).toContain("Older messages stand behind this door");
  expect(html).toContain("newest 1 message.");
});

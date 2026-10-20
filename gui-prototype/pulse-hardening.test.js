import { afterEach, beforeEach, expect, test } from "bun:test";
import { handlePulseClick, initPulse, renderHousePulse } from "./pulse.js";

const LANES = ["hardening alpha", "hardening beta"];
const TRACES = ["trace-alpha", "trace-beta"];
const ROOM = "hardening-pulse";
const STAMP = "2026-10-01T12:00:00Z";

function span(traceId, spanId = traceId) {
  return {
    traceId, spanId, component: "hardening", operation: "alpha",
    phase: "end", outcomeClass: "ok", observedAt: STAMP,
    durationUs: 100, dropCount: 0
  };
}

function answer(rows) {
  return Response.json({
    rows, room: ROOM, start: STAMP, end: STAMP, window: "24h",
    queryName: "hardening", queryVersion: 1, truncated: false
  });
}

function lateAnswer(outcome) {
  return outcome === "success"
    ? answer([span(TRACES[0], "retired-span")])
    : Response.json({ error: "retired-request-refused" }, { status: 403 });
}

function controlSelector(attribute, value) {
  return value == null ? `[${attribute}]` : `[${attribute}="${CSS.escape(value)}"]`;
}

async function inspectPanel(html, makeControl) {
  const view = {
    lanes: [], openLanes: [], choices: [], selectedTraces: [], traceSpanIds: [],
    spansDrawers: 0, traceDrawers: 0, spansTones: [], traceTones: [],
    spansMessage: "", traceMessage: "", controls: new Map()
  };

  await new HTMLRewriter()
    .on("[data-pulse-refresh], [data-pulse-lane], [data-pulse-span-trace]", {
      element(element) {
        const attributes = Object.fromEntries(element.attributes);
        const control = makeControl(attributes);
        view.controls.set(control.selector, control);

        if (attributes["data-pulse-lane"]) view.lanes.push(attributes["data-pulse-lane"]);
        if (attributes["data-pulse-span-trace"]) {
          view.choices.push(attributes["data-pulse-span-trace"]);

          if (attributes["aria-pressed"] === "true") view.selectedTraces.push(attributes["data-pulse-span-trace"]);
        }
      }
    })
    .on("details[open] > summary[data-pulse-lane]", {
      element(element) { view.openLanes.push(element.getAttribute("data-pulse-lane")); }
    })
    .on('section[aria-label="Lane spans"]', {
      element() { view.spansDrawers += 1; }
    })
    .on('section[aria-label="Lane trace"]', {
      element() { view.traceDrawers += 1; }
    })
    .on('section[aria-label="Lane spans"] .mechanics-snapshot-status [data-tone]', {
      element(element) { view.spansTones.push(element.getAttribute("data-tone")); }
    })
    .on('section[aria-label="Lane trace"] .mechanics-snapshot-status [data-tone]', {
      element(element) { view.traceTones.push(element.getAttribute("data-tone")); }
    })
    .on('section[aria-label="Lane spans"] > p', {
      text(chunk) { view.spansMessage += chunk.text; }
    })
    .on('section[aria-label="Lane trace"] > p', {
      text(chunk) { view.traceMessage += chunk.text; }
    })
    .on('section[aria-label="Lane trace"] .pulse-trace-spans .mechanics-row-title code', {
      element() { view.traceSpanIds.push(""); },
      text(chunk) { view.traceSpanIds[view.traceSpanIds.length - 1] += chunk.text; }
    })
    .transform(new Response(html))
    .text();

  return view;
}

function createPanel() {
  const globals = ["fetch", "window", "document", "CSS", "location"];
  const originals = globals.map(name => [name, Object.getOwnPropertyDescriptor(globalThis, name)]);
  const requests = [];
  const frames = new Map();
  const focused = [];
  const body = {};
  let nextFrame = 0;
  let view = { controls: new Map() };
  let rendered = Promise.resolve();
  const document = { body, activeElement: body, querySelector: selector => view.controls.get(selector) ?? null };
  const outsideControl = { selector: "[data-hardening-outside]", focus() { document.activeElement = outsideControl; } };

  Object.assign(globalThis, {
    document,
    location: { search: `?room=${ROOM}` },
    // Fixture identities contain only letters, hyphens, and the lane separator.
    CSS: { escape: value => value.replaceAll(" ", "\\ ") },
    window: {
      requestAnimationFrame(callback) {
        const id = ++nextFrame;
        frames.set(id, callback);

        return id;
      },
      cancelAnimationFrame(id) { frames.delete(id); }
    },
    fetch(path, options) {
      return new Promise(resolve => {
        requests.push({ path, body: JSON.parse(options.body), resolve, taken: false, settled: false });
      });
    }
  });

  function makeControl(attributes) {
    const attribute = ["data-pulse-refresh", "data-pulse-lane", "data-pulse-span-trace"]
      .find(name => Object.hasOwn(attributes, name));
    const selector = controlSelector(attribute, attributes[attribute] || null);
    const node = {
      selector,
      isConnected: true,
      dataset: {
        pulseLane: attributes["data-pulse-lane"],
        pulseOperation: attributes["data-pulse-operation"],
        pulseSpanKey: attributes["data-pulse-span-key"],
        pulseSpanTrace: attributes["data-pulse-span-trace"]
      },
      closest(candidate) { return candidate === `[${attribute}]` ? node : null; },
      focus() {
        if (!node.isConnected) return;

        document.activeElement = node;
        focused.push(selector);
      }
    };

    return node;
  }

  function render() {
    const html = renderHousePulse();
    rendered = rendered.then(async () => {
      const next = await inspectPanel(html, makeControl);

      for (const control of view.controls.values()) {
        control.isConnected = false;

        if (document.activeElement === control) document.activeElement = body;
      }

      view = next;
    });
  }

  async function drain() {
    await new Promise(resolve => setImmediate(resolve));
    await rendered;
  }

  async function activate(selector) {
    await rendered;
    const control = view.controls.get(selector);

    expect(control).toBeDefined();
    document.activeElement = control;
    expect(handlePulseClick({ target: control, preventDefault() {} })).toBe(true);
    await rendered;
  }

  function request(path) {
    const pending = requests.find(item => !item.taken);

    expect(pending?.path).toBe(path);
    pending.taken = true;

    return pending;
  }

  async function reply(pending, response) {
    expect(pending.settled).toBe(false);
    pending.settled = true;
    pending.resolve(response);
    await drain();
  }

  async function frame() {
    const callbacks = [...frames.values()];
    frames.clear();

    for (const callback of callbacks) callback(0);

    await rendered;
  }

  async function flushFrames() {
    while (frames.size) await frame();
  }

  const panel = {
    activate,
    lane: key => activate(controlSelector("data-pulse-lane", key)),
    trace: id => activate(controlSelector("data-pulse-span-trace", id)),
    request,
    reply,
    frame,
    flushFrames,
    focused,
    activeSelector: () => document.activeElement.selector ?? null,
    focusOutside: () => outsideControl.focus(),
    snapshot: async () => { await rendered; return view; },
    async boot() {
      initPulse({ requestRender: render });
      render();
      await activate("[data-pulse-refresh]");
      await reply(request("/live/insula/vitals"), answer(["alpha", "beta"].map(operation => ({
        component: "hardening", operation, phase: "end", outcomeClass: "ok",
        eventCount: 1, durationUsMax: 100, tokensInSum: 0, tokensOutSum: 0, dropCountSum: 0
      }))));
      await reply(request("/live/insula/retention"), answer([]));

      for (const key of view.openLanes) await panel.lane(key);

      // Reset retained module state through the same open/close path as the operator.
      await panel.lane(LANES[0]);
      await reply(request("/live/insula/spans"), answer([]));
      await panel.lane(LANES[0]);
      await flushFrames();
      focused.length = 0;
    },
    async dispose() {
      try {
        for (const pending of requests) {
          if (pending.settled) continue;

          pending.settled = true;
          pending.resolve(answer([]));
        }

        await drain();
      } finally {
        frames.clear();

        for (const [name, descriptor] of originals) {
          if (descriptor) Object.defineProperty(globalThis, name, descriptor);
          else delete globalThis[name];
        }
      }
    }
  };

  return panel;
}

let panel;
beforeEach(async () => {
  panel = createPanel();
  await panel.boot();
});
afterEach(async () => { await panel?.dispose(); });

async function openChoices() {
  await panel.lane(LANES[0]);
  await panel.reply(panel.request("/live/insula/spans"), answer(TRACES.map(id => span(id))));
  const view = await panel.snapshot();

  expect(view.openLanes).toEqual([LANES[0]]);
  expect(view.choices).toEqual(TRACES);
}

async function expectCurrentSpans(key) {
  const view = await panel.snapshot();

  expect(view.lanes).toEqual(LANES);
  expect(view.openLanes).toEqual([key]);
  expect(view.spansDrawers).toBe(1);
  expect(view.choices).toEqual(["current-trace"]);
  expect(view.spansTones).toEqual(["steady"]);
}

async function expectCurrentTrace(id) {
  const view = await panel.snapshot();

  expect(view.openLanes).toEqual([LANES[0]]);
  expect(view.choices).toEqual(TRACES);
  expect(view.selectedTraces).toEqual([id]);
  expect(view.traceDrawers).toBe(1);
  expect(view.traceSpanIds).toEqual(["current-span"]);
  expect(view.traceTones).toEqual(["steady"]);
}

for (const outcome of ["success", "refusal"]) {
  for (const revisit of [false, true]) {
    const journey = revisit ? "A to B to A" : "A to B";

    test(`hardening: spans ${journey} ignores a retired ${outcome}`, async () => {
      await panel.lane(LANES[0]);
      const retired = panel.request("/live/insula/spans");
      await panel.lane(LANES[1]);
      let current = panel.request("/live/insula/spans");
      let key = LANES[1];

      if (revisit) {
        await panel.reply(current, answer([span("current-trace")]));
        await expectCurrentSpans(LANES[1]);
        await panel.lane(LANES[0]);
        current = panel.request("/live/insula/spans");
        key = LANES[0];
      }

      await panel.reply(current, answer([span("current-trace")]));
      await expectCurrentSpans(key);
      await panel.reply(retired, lateAnswer(outcome));
      await expectCurrentSpans(key);
    });

    test(`hardening: trace ${journey} ignores a retired ${outcome}`, async () => {
      await openChoices();
      await panel.trace(TRACES[0]);
      const retired = panel.request("/live/insula/trace");
      await panel.trace(TRACES[1]);
      let current = panel.request("/live/insula/trace");
      let id = TRACES[1];

      if (revisit) {
        await panel.reply(current, answer([span(TRACES[1], "current-span")]));
        await expectCurrentTrace(TRACES[1]);
        await panel.trace(TRACES[0]);
        current = panel.request("/live/insula/trace");
        id = TRACES[0];
      }

      await panel.reply(current, answer([span(id, "current-span")]));
      await expectCurrentTrace(id);
      await panel.reply(retired, lateAnswer(outcome));
      await expectCurrentTrace(id);
    });
  }

  for (const transition of ["switch", "reopen"]) {
    test(`hardening: spans ${transition} keeps its replacement pending after a retired ${outcome}`, async () => {
      await panel.lane(LANES[0]);
      const retired = panel.request("/live/insula/spans");
      const key = transition === "switch" ? LANES[1] : LANES[0];

      if (transition === "reopen") await panel.lane(LANES[0]);

      await panel.lane(key);
      const current = panel.request("/live/insula/spans");
      const pending = await panel.snapshot();

      expect(pending.lanes).toEqual(LANES);
      expect(pending.openLanes).toEqual([key]);
      expect(pending.spansDrawers).toBe(1);
      expect(pending.spansTones).toEqual(["quiet"]);
      expect(pending.spansMessage).toContain(`settled spans of ${transition === "switch" ? "beta" : "alpha"}`);
      expect(pending.choices).toEqual([]);

      await panel.reply(retired, lateAnswer(outcome));
      const view = await panel.snapshot();

      expect(view.openLanes).toEqual([key]);
      expect(view.spansDrawers).toBe(1);
      expect(view.spansTones).toEqual(["quiet"]);
      expect(view.spansMessage).toBe(pending.spansMessage);
      expect(view.choices).toEqual([]);

      await panel.reply(current, answer([span("current-trace")]));
      await expectCurrentSpans(key);
    });

    test(`hardening: trace ${transition} keeps its replacement pending after a retired ${outcome}`, async () => {
      await openChoices();
      await panel.trace(TRACES[0]);
      const retired = panel.request("/live/insula/trace");
      const id = transition === "switch" ? TRACES[1] : TRACES[0];

      if (transition === "reopen") await panel.trace(TRACES[0]);

      await panel.trace(id);
      const current = panel.request("/live/insula/trace");
      const pending = await panel.snapshot();

      expect(pending.openLanes).toEqual([LANES[0]]);
      expect(pending.choices).toEqual(TRACES);
      expect(pending.selectedTraces).toEqual([id]);
      expect(pending.traceDrawers).toBe(1);
      expect(pending.traceTones).toEqual(["quiet"]);
      expect(pending.traceMessage).toContain(`Asked the Host for trace ${id}.`);
      expect(pending.traceSpanIds).toEqual([]);

      await panel.reply(retired, lateAnswer(outcome));
      const view = await panel.snapshot();

      expect(view.openLanes).toEqual([LANES[0]]);
      expect(view.choices).toEqual(TRACES);
      expect(view.selectedTraces).toEqual([id]);
      expect(view.traceDrawers).toBe(1);
      expect(view.traceTones).toEqual(["quiet"]);
      expect(view.traceMessage).toBe(pending.traceMessage);
      expect(view.traceSpanIds).toEqual([]);

      await panel.reply(current, answer([span(id, "current-span")]));
      await expectCurrentTrace(id);
    });
  }

  test(`hardening: a closed lane ignores its pending spans ${outcome}`, async () => {
    await panel.lane(LANES[0]);
    const retired = panel.request("/live/insula/spans");
    await panel.lane(LANES[0]);
    let view = await panel.snapshot();

    expect(view.lanes).toEqual(LANES);
    expect(view.openLanes).toEqual([]);
    expect(view.spansDrawers).toBe(0);

    await panel.reply(retired, lateAnswer(outcome));
    view = await panel.snapshot();

    expect(view.lanes).toEqual(LANES);
    expect(view.openLanes).toEqual([]);
    expect(view.spansDrawers).toBe(0);
  });

  test(`hardening: a closed trace ignores its pending ${outcome}`, async () => {
    await openChoices();
    await panel.trace(TRACES[0]);
    const retired = panel.request("/live/insula/trace");
    await panel.trace(TRACES[0]);
    let view = await panel.snapshot();

    expect(view.openLanes).toEqual([LANES[0]]);
    expect(view.choices).toEqual(TRACES);
    expect(view.selectedTraces).toEqual([]);
    expect(view.traceDrawers).toBe(0);

    await panel.reply(retired, lateAnswer(outcome));
    view = await panel.snapshot();

    expect(view.openLanes).toEqual([LANES[0]]);
    expect(view.choices).toEqual(TRACES);
    expect(view.selectedTraces).toEqual([]);
    expect(view.traceDrawers).toBe(0);
  });

  for (const next of ["close", "switch"]) {
    test(`hardening: a parent lane ${next} retires a pending trace ${outcome}`, async () => {
      await openChoices();
      await panel.trace(TRACES[0]);
      const retired = panel.request("/live/insula/trace");
      await panel.lane(next === "close" ? LANES[0] : LANES[1]);

      if (next === "switch") {
        await panel.reply(panel.request("/live/insula/spans"), answer([span("sibling-trace")]));
      }

      await panel.reply(retired, lateAnswer(outcome));
      const view = await panel.snapshot();

      expect(view.lanes).toEqual(LANES);
      expect(view.openLanes).toEqual(next === "close" ? [] : [LANES[1]]);
      expect(view.choices).toEqual(next === "close" ? [] : ["sibling-trace"]);
      expect(view.traceDrawers).toBe(0);
      expect(view.selectedTraces).toEqual([]);
    });
  }
}

test("hardening: current spans and trace refusals remain observable", async () => {
  await panel.lane(LANES[0]);
  await panel.reply(panel.request("/live/insula/spans"), Response.json({ error: "current-spans-refused" }, { status: 403 }));
  let view = await panel.snapshot();

  expect(view.lanes).toEqual(LANES);
  expect(view.openLanes).toEqual([LANES[0]]);
  expect(view.spansDrawers).toBe(1);
  expect(view.spansTones).toEqual(["attention"]);
  expect(view.spansMessage).toContain("current-spans-refused");

  await panel.lane(LANES[0]);
  await openChoices();
  await panel.trace(TRACES[0]);
  await panel.reply(panel.request("/live/insula/trace"), Response.json({ error: "current-trace-refused" }, { status: 403 }));
  view = await panel.snapshot();

  expect(view.choices).toEqual(TRACES);
  expect(view.selectedTraces).toEqual([TRACES[0]]);
  expect(view.traceDrawers).toBe(1);
  expect(view.traceTones).toEqual(["attention"]);
  expect(view.traceMessage).toContain("current-trace-refused");
});

test("hardening: a newer lane action retires deferred focus on the previous lane", async () => {
  await panel.lane(LANES[0]);
  await panel.frame();
  await panel.lane(LANES[1]);
  await panel.frame();

  expect(panel.focused).not.toContain(controlSelector("data-pulse-lane", LANES[0]));

  await panel.flushFrames();

  expect(panel.activeSelector()).toBe(controlSelector("data-pulse-lane", LANES[1]));
});

test("hardening: a newer trace action retires deferred focus on the previous span", async () => {
  await openChoices();
  await panel.flushFrames();
  panel.focused.length = 0;
  await panel.trace(TRACES[0]);
  await panel.frame();
  await panel.trace(TRACES[1]);
  await panel.frame();

  expect(panel.focused).not.toContain(controlSelector("data-pulse-span-trace", TRACES[0]));

  await panel.flushFrames();

  expect(panel.activeSelector()).toBe(controlSelector("data-pulse-span-trace", TRACES[1]));
});

test("hardening: selecting a span retires deferred focus on its lane header", async () => {
  await openChoices();
  await panel.frame();
  await panel.trace(TRACES[0]);
  await panel.frame();

  expect(panel.focused).not.toContain(controlSelector("data-pulse-lane", LANES[0]));

  await panel.flushFrames();

  expect(panel.activeSelector()).toBe(controlSelector("data-pulse-span-trace", TRACES[0]));
});

test("hardening: current open and close actions retain their focus target", async () => {
  await openChoices();
  await panel.flushFrames();

  expect(panel.activeSelector()).toBe(controlSelector("data-pulse-lane", LANES[0]));

  await panel.trace(TRACES[0]);
  await panel.reply(panel.request("/live/insula/trace"), answer([span(TRACES[0], "current-span")]));
  await panel.flushFrames();

  expect(panel.activeSelector()).toBe(controlSelector("data-pulse-span-trace", TRACES[0]));

  await panel.trace(TRACES[0]);
  await panel.flushFrames();

  expect(panel.activeSelector()).toBe(controlSelector("data-pulse-span-trace", TRACES[0]));
  expect((await panel.snapshot()).traceDrawers).toBe(0);

  await panel.lane(LANES[0]);
  await panel.flushFrames();

  expect(panel.activeSelector()).toBe(controlSelector("data-pulse-lane", LANES[0]));
  expect((await panel.snapshot()).openLanes).toEqual([]);
});

for (const kind of ["spans", "trace"]) {
  test(`hardening: a deferred ${kind} focus callback cannot reclaim unrelated focus between frames`, async () => {
    if (kind === "trace") {
      await openChoices();
      await panel.flushFrames();
      await panel.trace(TRACES[0]);
    } else {
      await panel.lane(LANES[0]);
    }

    const current = panel.request(`/live/insula/${kind}`);
    const owner = kind === "trace"
      ? controlSelector("data-pulse-span-trace", TRACES[0])
      : controlSelector("data-pulse-lane", LANES[0]);
    await panel.flushFrames();

    expect(panel.activeSelector()).toBe(owner);

    panel.focused.length = 0;
    await panel.reply(current, answer([span(TRACES[0], "current-span")]));
    await panel.frame();
    panel.focusOutside();

    expect(panel.activeSelector()).toBe("[data-hardening-outside]");

    await panel.frame();
    await panel.flushFrames();
    const view = await panel.snapshot();

    expect(view.openLanes).toEqual([LANES[0]]);
    expect(kind === "trace" ? view.traceDrawers : view.spansDrawers).toBe(1);
    expect(kind === "trace" ? view.traceSpanIds : view.choices)
      .toEqual(kind === "trace" ? ["current-span"] : [TRACES[0]]);
    expect(panel.focused).toEqual([]);
    expect(panel.activeSelector()).toBe("[data-hardening-outside]");
  });

  for (const outcome of ["success", "refusal"]) {
    test(`hardening: a current ${kind} ${outcome} cannot take focus from an unrelated control`, async () => {
      if (kind === "trace") {
        await openChoices();
        await panel.flushFrames();
        await panel.trace(TRACES[0]);
      } else {
        await panel.lane(LANES[0]);
      }

      const current = panel.request(`/live/insula/${kind}`);
      const owner = kind === "trace"
        ? controlSelector("data-pulse-span-trace", TRACES[0])
        : controlSelector("data-pulse-lane", LANES[0]);
      await panel.flushFrames();

      expect(panel.activeSelector()).toBe(owner);

      panel.focusOutside();

      expect(panel.activeSelector()).toBe("[data-hardening-outside]");

      const response = outcome === "success"
        ? answer([span(TRACES[0], "current-span")])
        : Response.json({ error: "current-request-refused" }, { status: 403 });
      await panel.reply(current, response);
      await panel.flushFrames();
      const view = await panel.snapshot();

      expect(view.openLanes).toEqual([LANES[0]]);
      expect(kind === "trace" ? view.traceDrawers : view.spansDrawers).toBe(1);

      if (outcome === "success") {
        expect(kind === "trace" ? view.traceSpanIds : view.choices)
          .toEqual(kind === "trace" ? ["current-span"] : [TRACES[0]]);
      } else {
        expect(kind === "trace" ? view.traceTones : view.spansTones).toEqual(["attention"]);
        expect(kind === "trace" ? view.traceMessage : view.spansMessage).toContain("current-request-refused");
      }

      expect(panel.activeSelector()).toBe("[data-hardening-outside]");
    });
  }
}

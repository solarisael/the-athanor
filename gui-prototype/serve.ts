// Local serving harness for the GUI prototype — the one live wire.
//
// Serves the static prototype files AND proxies read-only House queries to one
// room Host, holding the bearer token server-side so the page never sees a
// credential. Operator ruling 2026-08-20 (Sol): the prototype may read the
// actual Athanor; it still writes nothing.
//
// Run: bun gui-prototype/serve.ts   (PULSE_ROOM=kodo PULSE_PORT=4175)

const ROOT = import.meta.dir;
const CONFIG_PATH = "C:/ProgramData/Solarisael/Athanor/config/runtime.json";
const SECRETS_PATH = "C:/ProgramData/Solarisael/Athanor/secrets/runtime-secrets.json";

// Named doors, never a pattern: every proxied path is written here beside the
// exact upstream it reaches, so no upstream opens by accident and a typo is a
// 404 rather than a surprise. POST only, bearer added on this side.
const LIVE_ROUTES = new Map([
  ["/live/insula/vitals", "/athanor/v1/insula/vitals"],
  ["/live/insula/retention", "/athanor/v1/insula/retention"],
  ["/live/docket/board", "/athanor/v1/docket/board"],
  ["/live/docket/evidence", "/athanor/v1/docket/evidence"],
  ["/live/hallway/inbox", "/athanor/v1/hallway/inbox"],
  ["/live/memory/timeline", "/athanor/v1/memory/timeline"],
  ["/live/memory/read", "/athanor/v1/memory/read"],
  ["/live/lesson/timeline", "/athanor/v1/lesson/timeline"],
]);

// The three durable doors above are mapped but not yet cut upstream: as of
// 2026-08-22 the kodo Host answers all three with 404 and another room owns the
// cut. That 404 is therefore the upstream's answer and not a typo here, which
// is why the page must tell the two apart — an unmapped path never reaches
// fetch and returns this proxy's own "unknown live route" body instead.

const runtime = await Bun.file(CONFIG_PATH).json();
const secrets = await Bun.file(SECRETS_PATH).json();
const room = Bun.env.PULSE_ROOM ?? "kodo";
const port = Number(Bun.env.PULSE_PORT ?? 4175);
const host = runtime.rooms.find((entry: { room: string }) => entry.room === room);
if (!host) throw new Error(`room ${room} is not in runtime.json`);

Bun.serve({
  hostname: "127.0.0.1",
  port,
  async fetch(request) {
    const url = new URL(request.url);

    if (url.pathname.startsWith("/live/")) {
      const upstreamPath = LIVE_ROUTES.get(url.pathname);
      if (!upstreamPath) return new Response("unknown live route", { status: 404 });
      if (request.method !== "POST") return new Response("POST only", { status: 405 });

      const upstream = await fetch(`http://127.0.0.1:${host.port}${upstreamPath}`, {
        method: "POST",
        headers: {
          "content-type": "application/json",
          authorization: `Bearer ${secrets.hostToken}`,
        },
        body: await request.text(),
      });
      return new Response(await upstream.text(), {
        status: upstream.status,
        headers: { "content-type": "application/json" },
      });
    }

    const path = url.pathname === "/" ? "/index.html" : url.pathname;
    if (path.includes("..")) return new Response("refused", { status: 400 });
    const file = Bun.file(`${ROOT}${path}`);
    if (!(await file.exists())) return new Response("not found", { status: 404 });
    return new Response(file);
  },
});

console.log(`prototype on http://127.0.0.1:${port} · live House reads via ${room} Host :${host.port}`);

// Local serving harness for the GUI prototype — the one live wire.
//
// Serves the static prototype files AND proxies read-only House queries to one
// room Host, holding the bearer token server-side so the page never sees a
// credential. Operator ruling 2026-08-20 (Sol): the prototype may read the
// actual Athanor; it still writes nothing.
//
// Run: bun gui-prototype/serve.ts   (PULSE_ROOM=kodo PULSE_PORT=4175)
//
// PULSE_HOST_PORT overrides the installed `runtime.json` hostPort. A Host route
// cannot be proven on the rendered surface before it is deployed, because the
// installed Host does not carry it yet; this lets the harness read a locally
// built Host on a spare port instead. The room, the bearer, and the allow-list
// are unchanged, so nothing else about the one live wire moves.

const ROOT = import.meta.dir;
const CONFIG_PATH = "C:/ProgramData/Solarisael/Athanor/config/runtime.json";
const SECRETS_PATH = "C:/ProgramData/Solarisael/Athanor/secrets/runtime-secrets.json";

// [gui/prototype/proxy] [security/allowlist]
//
// One page-facing path per Host call the prototype may perform, each carrying
// the method its Host route actually answers. The table lives in
// live-routes.json so the desktop app (gui-desktop) and this dev server hold
// one allow-list. The page always POSTs; a GET route upstream is called
// without a body. Nothing outside this map is reachable, and the bearer never
// leaves this process.
const LIVE_ROUTES = new Map<string, { path: string; method: string }>(
  Object.entries(await Bun.file(`${ROOT}/live-routes.json`).json()),
);

const runtime = await Bun.file(CONFIG_PATH).json();
const secrets = await Bun.file(SECRETS_PATH).json();
const room = Bun.env.PULSE_ROOM ?? "kodo";
const port = Number(Bun.env.PULSE_PORT ?? 4175);
const hostPort = Number(Bun.env.PULSE_HOST_PORT ?? runtime.hostPort);
if (!Number.isInteger(hostPort)) throw new Error("no usable hostPort");
if (!runtime.rooms.some((entry: { room: string }) => entry.room === room)) {
  throw new Error(`room ${room} is not in runtime.json`);
}
const roomPath = `/room/${room}`;

Bun.serve({
  hostname: "127.0.0.1",
  port,
  async fetch(request) {
    const url = new URL(request.url);

    if (url.pathname.startsWith("/live/")) {
      const route = LIVE_ROUTES.get(url.pathname);
      if (!route) return new Response("unknown live route", { status: 404 });
      if (request.method !== "POST") return new Response("POST only", { status: 405 });

      // Parity with gui-desktop/src/proxy.rs: a transport failure is 502 with
      // the hop named; a Host error passes through with its own status.
      let upstream: Response;
      try {
        upstream = await fetch(`http://127.0.0.1:${hostPort}${roomPath}${route.path}`, {
          method: route.method,
          headers: {
            "content-type": "application/json",
            authorization: `Bearer ${secrets.hostToken}`,
          },
          body: route.method === "GET" ? undefined : await request.text(),
          signal: AbortSignal.timeout(20_000),
        });
      } catch (error) {
        const hop = error instanceof Error && error.name === "TimeoutError" ? "host_timeout" : "host_unreachable";
        return Response.json({ error: `Host request failed: ${error instanceof Error ? error.message : String(error)}`, hop }, { status: 502 });
      }
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

console.log(`prototype on http://127.0.0.1:${port} · live House reads via ${roomPath} on Host :${hostPort}`);

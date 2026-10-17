// Local serving harness for the GUI prototype — the one live wire.
//
// The proxy holds the Host bearer outside the page. Browser requests must
// belong to this loopback origin before they can reach any proxy route.
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
const rooms = [...new Set<string>(runtime.rooms.map((entry: { room: string }) => entry.room))];
if (!rooms.includes(room)) {
  throw new Error(`room ${room} is not in runtime.json`);
}
const roomPath = `/room/${encodeURIComponent(room)}`;

Bun.serve({
  hostname: "127.0.0.1",
  port,
  async fetch(request, server) {
    const authority = `127.0.0.1:${server.port}`;
    const origin = request.headers.get("origin");
    const unsafe = request.method !== "GET" && request.method !== "HEAD";
    if (
      request.headers.get("host") !== authority ||
      (origin !== null && origin !== `http://${authority}`) ||
      (unsafe && origin === null) ||
      request.headers.get("sec-fetch-site") === "cross-site"
    ) return new Response("forbidden origin", { status: 403 });
    const url = new URL(request.url);

    if (url.pathname.startsWith("/local/")) {
      if (!["/local/rooms", "/local/repair/status", "/local/repair/start"].includes(url.pathname)) {
        return new Response("unknown local route", { status: 404 });
      }
      if (request.method !== "POST") return new Response("POST only", { status: 405 });
      if (url.pathname !== "/local/rooms") {
        return Response.json({ error: "Local repair requires the desktop app", hop: "repair" }, { status: 501 });
      }
      try {
        const text = await request.text();
        const body = text.trim() === "" ? {} : JSON.parse(text);
        if (body === null || typeof body !== "object" || Array.isArray(body) || Object.keys(body).length !== 0) {
          return new Response("no body fields allowed", { status: 400 });
        }
      } catch {
        return new Response("invalid JSON body", { status: 400 });
      }
      return Response.json({ defaultRoom: room, rooms, capabilities: { repair: false } });
    }

    if (url.pathname.startsWith("/live/")) {
      const route = LIVE_ROUTES.get(url.pathname);
      if (!route) return new Response("unknown live route", { status: 404 });
      if (request.method !== "POST") return new Response("POST only", { status: 405 });
      const selectedRoom = request.headers.get("x-athanor-room") ?? room;
      if (!rooms.includes(selectedRoom)) return new Response("unknown room", { status: 403 });
      const selectedRoomPath = `/room/${encodeURIComponent(selectedRoom)}`;
      let body: string | undefined;
      if (route.method !== "GET") {
        try {
          body = await request.text();
          if (url.pathname === "/live/session") {
            const session = JSON.parse(body);
            if (session === null || typeof session !== "object" || Array.isArray(session)) {
              return new Response("session body must be a JSON object", { status: 400 });
            }
            if (!["status", "command", "commandStatus"].includes(session.action)) {
              return new Response("session action refused", { status: 403 });
            }
            body = JSON.stringify(session);
          }
        } catch {
          return new Response("invalid JSON body", { status: 400 });
        }
      }

      // Parity with gui-desktop/src/proxy.rs: a transport failure is 502 with
      // the hop named; a Host error passes through with its own status.
      let upstream: Response;
      try {
        upstream = await fetch(`http://127.0.0.1:${hostPort}${selectedRoomPath}${route.path}`, {
          method: route.method,
          headers: {
            "content-type": "application/json",
            authorization: `Bearer ${secrets.hostToken}`,
          },
          body,
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

console.log(`prototype on http://127.0.0.1:${port} · live House via ${roomPath} on Host :${hostPort}`);

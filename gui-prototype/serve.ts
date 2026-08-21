// Local serving harness for the GUI prototype — the one live wire.
//
// Serves the three static prototype files AND proxies read-only Insula
// queries to one room Host, holding the bearer token server-side so the
// page never sees a credential. Operator ruling 2026-08-20 (Sol): the
// prototype may read the actual Athanor; it still writes nothing.
//
// Run: bun gui-prototype/serve.ts   (PULSE_ROOM=kodo PULSE_PORT=4175)

const ROOT = import.meta.dir;
const CONFIG_PATH = "C:/ProgramData/Solarisael/Athanor/config/runtime.json";
const SECRETS_PATH = "C:/ProgramData/Solarisael/Athanor/secrets/runtime-secrets.json";
const LIVE_ROUTES = new Set(["vitals", "retention"]);

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

    const live = url.pathname.match(/^\/live\/insula\/([a-z]+)$/);
    if (live) {
      if (!LIVE_ROUTES.has(live[1])) return new Response("unknown live route", { status: 404 });
      if (request.method !== "POST") return new Response("POST only", { status: 405 });
      const upstream = await fetch(`http://127.0.0.1:${host.port}/athanor/v1/insula/${live[1]}`, {
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

console.log(`prototype on http://127.0.0.1:${port} · live Insula reads via ${room} Host :${host.port}`);

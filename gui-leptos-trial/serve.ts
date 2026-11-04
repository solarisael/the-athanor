// Local door for the Leptos trial: Trunk output plus one live Host read.
//
// The Host bearer stays in this process. The page reaches the Host only
// through POST /live/health. The guards match gui-prototype/serve.ts and
// gui-desktop/src/proxy.rs, so the trial does not open a weaker door.
//
// Run: bun gui-leptos-trial/serve.ts   (optional PULSE_ROOM; default kintsu)

const DIST = `${import.meta.dir}/dist`;
const CONFIG_PATH = "C:/ProgramData/Solarisael/Athanor/config/runtime.json";
const SECRETS_PATH = "C:/ProgramData/Solarisael/Athanor/secrets/runtime-secrets.json";
const PORT = 4186;
const AUTHORITY = `127.0.0.1:${PORT}`;
const ORIGIN = `http://${AUTHORITY}`;

const runtime = await Bun.file(CONFIG_PATH).json();
const secrets = await Bun.file(SECRETS_PATH).json();

const room = Bun.env.PULSE_ROOM ?? "kintsu";
const rooms = new Set<string>(runtime.rooms.map((entry: { room: string }) => entry.room));
if (!rooms.has(room)) throw new Error(`room ${room} is not in runtime.json`);

const hostPort = runtime.hostPort;
if (!Number.isInteger(hostPort) || hostPort < 1 || hostPort > 65535) {
  throw new Error("runtime.json has no usable hostPort");
}
if (typeof secrets.hostToken !== "string" || secrets.hostToken === "") {
  throw new Error("runtime-secrets.json has no hostToken");
}
const HEALTH_URL = `http://127.0.0.1:${hostPort}/room/${encodeURIComponent(room)}/health`;

// Trunk writes hashed top-level bundles; nothing nested or other-typed is served.
const BUNDLE = /^[A-Za-z0-9_-][A-Za-z0-9_.-]*\.(?:js|wasm)$/;

function staticName(pathname: string): string | null {
  let name: string;
  try {
    name = decodeURIComponent(pathname).slice(1);
  } catch {
    return null;
  }
  if (name === "") return "index.html";
  if (name.includes("..")) return null;
  if (name === "index.html" || name === "styles.css" || BUNDLE.test(name)) return name;
  return null;
}

async function health(request: Request): Promise<Response> {
  const roomHeader = request.headers.get("x-athanor-room");
  if (roomHeader !== null && roomHeader !== room) return new Response("unknown room", { status: 403 });

  let body: unknown;
  try {
    body = JSON.parse(await request.text());
  } catch {
    return new Response("invalid JSON body", { status: 400 });
  }
  if (body === null || typeof body !== "object" || Array.isArray(body) || Object.keys(body).length !== 0) {
    return new Response("body must be an empty JSON object", { status: 400 });
  }

  // A Host error keeps its own status; only a transport failure becomes 502.
  // The hop name is the whole error, so no upstream URL leaves this process.
  let upstream: Response;
  try {
    upstream = await fetch(HEALTH_URL, {
      method: "GET",
      headers: { authorization: `Bearer ${secrets.hostToken}` },
      redirect: "manual",
      signal: AbortSignal.timeout(20_000),
    });
  } catch (error) {
    const hop = error instanceof Error && error.name === "TimeoutError" ? "host_timeout" : "host_unreachable";
    return Response.json({ error: "Host request failed", hop }, { status: 502 });
  }
  return new Response(await upstream.text(), {
    status: upstream.status,
    headers: { "content-type": "application/json" },
  });
}

Bun.serve({
  hostname: "127.0.0.1",
  port: PORT,
  maxRequestBodySize: 1024,
  async fetch(request) {
    const origin = request.headers.get("origin");
    const unsafe = request.method !== "GET" && request.method !== "HEAD";
    if (
      request.headers.get("host") !== AUTHORITY ||
      (origin !== null && origin !== ORIGIN) ||
      (unsafe && origin === null) ||
      request.headers.get("sec-fetch-site") === "cross-site"
    ) return new Response("forbidden origin", { status: 403 });

    const { pathname } = new URL(request.url);
    if (pathname.startsWith("/live/")) {
      if (pathname !== "/live/health") return new Response("unknown live route", { status: 404 });
      if (request.method !== "POST") return new Response("POST only", { status: 405 });
      return health(request);
    }

    if (unsafe) return new Response("GET or HEAD only", { status: 405 });
    const name = staticName(pathname);
    if (name === null) return new Response("not found", { status: 404 });

    // Read from disk on every request, so a fresh Trunk build shows on reload.
    const file = Bun.file(`${DIST}/${name}`);
    if (!(await file.exists())) return new Response("not found", { status: 404 });
    const headers = { "content-type": file.type, "cache-control": "no-store" };
    if (request.method === "HEAD") {
      return new Response(null, { headers: { ...headers, "content-length": String(file.size) } });
    }
    return new Response(file, { headers });
  },
});

console.log(`leptos trial on ${ORIGIN} · Host health for room ${room}`);

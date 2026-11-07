import { basename, join } from "node:path";
import { readMap, staleFiles, type MapMeta } from "./stale.ts";

const bundle = await Bun.build({
  entrypoints: [join(import.meta.dir, "anatomy.js"), join(import.meta.dir, "constellation.js")],
  target: "browser",
  minify: true,
});
if (!bundle.success) throw new AggregateError(bundle.logs, "Anatomy viewer build failed");

const scripts = new Map(bundle.outputs.map(output => [`/${basename(output.path)}`, output]));
const layoutPath = join(import.meta.dir, "anatomy.layout.json");
const mapPath = join(import.meta.dir, "anatomy.jsonl");
const LAYOUT_MAX_BYTES = 1_000_000;

const files = new Map([
  ["/", join(import.meta.dir, "anatomy.html")],
  ["/anatomy.html", join(import.meta.dir, "anatomy.html")],
  ["/anatomy.css", join(import.meta.dir, "anatomy.css")],
  ["/anatomy.jsonl", join(import.meta.dir, "anatomy.jsonl")],
  ["/constellation", join(import.meta.dir, "constellation.html")],
  ["/constellation.html", join(import.meta.dir, "constellation.html")],
  ["/constellation.css", join(import.meta.dir, "constellation.css")],
  ["/anatomy.layout.json", layoutPath],
  ["/anatomy.overlay.jsonl", join(import.meta.dir, "anatomy.overlay.jsonl")],
]);

// Hashes are taken per request so a reload shows the drift of the current tree.
async function staleReport(): Promise<Response> {
  const meta = (await readMap(mapPath)).find((record) => record.type === "meta") as MapMeta;
  const files = Object.fromEntries(await staleFiles(join(import.meta.dir, ".."), meta));
  return Response.json({ revision: meta.revision, capturedAt: meta.capturedAt, files }, { headers: { "Cache-Control": "no-store" } });
}

// The server listens on loopback only. The one write it accepts is the layout file that the
// constellation saves after you move or resize a bubble.
async function saveLayout(request: Request): Promise<Response> {
  const text = await request.text();

  if (text.length > LAYOUT_MAX_BYTES) return new Response("Layout is too large", { status: 413 });
  try {
    JSON.parse(text);
  } catch {
    return new Response("Layout is not valid JSON", { status: 400 });
  }

  await Bun.write(layoutPath, text);
  return new Response(null, { status: 204 });
}

// The server answers only requests addressed to itself. A page on another name that resolves
// to 127.0.0.1 (DNS rebinding) carries its own Host header and gets 403.
const ALLOWED_HOSTS = new Set(["127.0.0.1:4187", "localhost:4187"]);

Bun.serve({
  hostname: "127.0.0.1",
  port: 4187,
  async fetch(request) {
    const pathname = new URL(request.url).pathname;

    if (!ALLOWED_HOSTS.has(request.headers.get("host") ?? "")) return new Response("Forbidden host", { status: 403 });

    if (request.method === "PUT" && pathname === "/anatomy.layout.json") return saveLayout(request);

    if (request.method !== "GET" && request.method !== "HEAD") {
      return new Response("Method not allowed", { status: 405 });
    }

    if (pathname === "/anatomy.stale.json") return staleReport();

    const script = scripts.get(pathname);
    if (script) {
      return new Response(script, {
        headers: { "Content-Type": "text/javascript", "Cache-Control": "no-store" },
      });
    }

    const path = files.get(pathname);
    if (!path) return new Response("Not found", { status: 404 });

    const file = Bun.file(path);
    if (!(await file.exists())) return new Response("Not found", { status: 404 });

    return new Response(file, {
      headers: { "Cache-Control": "no-store" },
    });
  },
});
console.log("Anatomy ready on http://127.0.0.1:4187 (constellation at /constellation)");

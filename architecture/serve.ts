import { join } from "node:path";

const bundle = await Bun.build({
  entrypoints: [join(import.meta.dir, "anatomy.js")],
  target: "browser",
  minify: true,
});
if (!bundle.success) throw new AggregateError(bundle.logs, "Anatomy viewer build failed");

const files = new Map([
  ["/", join(import.meta.dir, "anatomy.html")],
  ["/anatomy.html", join(import.meta.dir, "anatomy.html")],
  ["/anatomy.css", join(import.meta.dir, "anatomy.css")],
  ["/anatomy.jsonl", join(import.meta.dir, "anatomy.jsonl")],
]);

Bun.serve({
  hostname: "127.0.0.1",
  port: 4187,
  fetch(request) {
    if (request.method !== "GET" && request.method !== "HEAD") {
      return new Response("Method not allowed", { status: 405 });
    }
    const pathname = new URL(request.url).pathname;
    if (pathname === "/anatomy.js") {
      return new Response(bundle.outputs[0], {
        headers: { "Content-Type": "text/javascript", "Cache-Control": "no-store" },
      });
    }
    const path = files.get(pathname);
    if (!path) return new Response("Not found", { status: 404 });
    return new Response(Bun.file(path), {
      headers: { "Cache-Control": "no-store" },
    });
  },
});
console.log("Anatomy ready on http://127.0.0.1:4187");

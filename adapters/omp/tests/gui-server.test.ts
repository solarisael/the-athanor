import { describe, expect, test } from "bun:test";
import { EventEmitter } from "node:events";
import { PassThrough } from "node:stream";
import { createGuiServer, runCommand, startGuiServer } from "../gui-server.ts";

const fake = `process.stdin.setEncoding('utf8'); process.stdin.on('data', d => { for (const line of d.split('\\n')) { if (!line) continue; const x=JSON.parse(line); const error=x.params.fail ? {code:'DATABASE_WRITE_FAILED',message:'Write rolled back',retryable:true,details:{owner:{component:'substrate',path:'src/store.rs',symbol:'write_memory'},evidence:[{severity:'warning',summary:'transaction rolled back'}],targets:['src/store.rs#write_memory'],next_checks:[{action:'inspect',target:'src/store.rs#write_memory'}],execution:{request_dispatched:true,write_outcome:'rolled_back',retry:'safe_now'}}} : null; process.stdout.write(JSON.stringify(error ? {protocol:1,id:x.id,error} : {protocol:1,id:x.id,result:{ok:true,method:x.method,params:x.params}})+'\\n'); } });`;

describe("GUI server security boundary", () => {
  let handle: Awaited<ReturnType<typeof startGuiServer>>;
  test("binds localhost, emits CSP and serves assets", async () => {
    handle = await startGuiServer({ executable: process.execPath, args: ["-e", fake], cwd: import.meta.dir + "/.." });
    const response = await fetch(`http://127.0.0.1:${handle.port}/`);
    expect(response.status).toBe(200);
    expect(response.headers.get("content-security-policy")).toContain("default-src 'self'");
    expect(response.headers.get("cache-control")).toBe("no-store");
    const csrf = await (await fetch(`http://127.0.0.1:${handle.port}/api/csrf`)).json() as { token: string };
    expect(csrf.token).toBe(handle.csrfToken);
  });
  test("requires csrf and rejects disallowed RPC method", async () => {
    const url = `http://127.0.0.1:${handle.port}/api/rpc`;
    expect((await fetch(url, { method:"POST", body:"{}" })).status).toBe(403);
    const r = await fetch(url, { method:"POST", headers:{"x-csrf-token":handle.csrfToken,"content-type":"application/json","origin":`http://127.0.0.1:${handle.port}`}, body:JSON.stringify({method:"exec",params:{}}) });
    expect(r.status).toBe(400);
  });
  test("dispatches valid payloads and health uses operation check", async () => {
    const origin = `http://127.0.0.1:${handle.port}`;
    const rpc = await fetch(origin + "/api/rpc", { method: "POST", headers: {"x-csrf-token": handle.csrfToken, "content-type": "application/json", origin}, body: JSON.stringify({ method: "remember", params: { room: "house", kind: "memory", operation: "add", repNumber: 1, dryRun: true, title: "valid", body: "payload" } }) });
    expect(rpc.status).toBe(200);
    const payload = await rpc.json() as any;
    expect(payload.result.params).toEqual({ room: "house", kind: "memory", operation: "add", repNumber: 1, dryRun: true, title: "valid", body: "payload" });
    const health = await fetch(origin + "/api/health");
    expect(health.status).toBe(200);
    expect((await health.json() as any).cluster.params).toEqual({ operation: "check", room: "gui" });
  });
  test("preserves canonical diagnostic errors for the GUI", async () => {
    const origin = `http://127.0.0.1:${handle.port}`;
    const response = await fetch(origin + "/api/rpc", { method: "POST", headers: { "x-csrf-token": handle.csrfToken, "content-type": "application/json", origin }, body: JSON.stringify({ method: "remember", params: { fail: true } }) });
    expect(response.status).toBe(500);
    const payload = await response.json() as any;
    expect(payload.error).toMatchObject({
      code: "DATABASE_WRITE_FAILED",
      message: "Write rolled back",
      retryable: true,
      details: {
        owner: { component: "substrate", path: "src/store.rs", symbol: "write_memory" },
        evidence: [{ severity: "warning", summary: "transaction rolled back" }],
        targets: ["src/store.rs#write_memory"],
        next_checks: [{ action: "inspect", target: "src/store.rs#write_memory" }],
        execution: { request_dispatched: true, write_outcome: "rolled_back", retry: "safe_now" },
      },
    });
  });
  test("closes cleanly", async () => { await handle.close(); });
});

describe("GUI worker lifecycle", () => {
  test("command timeout rejects after the kill attempt without waiting for close", async () => {
    const child = Object.assign(new EventEmitter(), {
      stdout: new PassThrough(),
      stderr: new PassThrough(),
      pid: undefined,
    });
    let killed = false;
    await expect(runCommand("synthetic-command", [], undefined, 1, {
      spawnProcess: (() => child) as any,
      killProcessTree: async () => { killed = true; },
    })).rejects.toThrow("command timed out");
    expect(killed).toBe(true);
  });

  test("transport replacement waits for the failed worker exit receipt", async () => {
    let resolveClose: (() => void) | undefined;
    let factories = 0;
    const first = {
      request: async () => { throw new Error("transport failed"); },
      close: () => new Promise<void>(resolve => { resolveClose = resolve; }),
    };
    const second = {
      request: async () => ({ ok: true }),
      close: async () => {},
    };
    const handle = await startGuiServer({
      executable: "synthetic-worker",
      cwd: import.meta.dir + "/..",
      transportFactory: (() => (++factories === 1 ? first : second)) as any,
    });
    try {
      const origin = `http://127.0.0.1:${handle.port}`;
      const response = fetch(origin + "/api/rpc", {
        method: "POST",
        headers: {
          "x-csrf-token": handle.csrfToken,
          "content-type": "application/json",
          origin,
        },
        body: JSON.stringify({ method: "recall", params: {} }),
      });
      await Bun.sleep(5);
      expect(factories).toBe(1);
      expect(resolveClose).toBeDefined();
      resolveClose?.();
      expect((await response).status).toBe(500);
      expect(factories).toBe(2);
    } finally {
      await handle.close();
    }
  });

  test("GUI shutdown drains the HTTP server before closing its worker", async () => {
    const order: string[] = [];
    const transport = {
      request: async () => ({ ok: true }),
      close: async () => { order.push("transport"); },
    };
    const handle = createGuiServer({
      executable: "synthetic-worker",
      cwd: import.meta.dir + "/..",
      transportFactory: (() => transport) as any,
    });
    handle.server.close = ((callback?: (error?: Error) => void) => {
      order.push("server");
      callback?.();
      return handle.server;
    }) as any;
    await handle.close();
    expect(order).toEqual(["server", "transport"]);
  });
});

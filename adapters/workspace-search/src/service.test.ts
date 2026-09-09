import assert from "node:assert/strict";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { closeService, index, status } from "./service.ts";

test("a failed file can be repaired without a false incomplete-index error", async (t) => {
  const root = await mkdtemp(join(tmpdir(), "athanor-index-recovery-"));
  let available = false;
  // The real zvec index owns the state transition. Only external embedding availability changes.
  t.mock.method(globalThis, "fetch", async (_url: string, init: RequestInit) => {
    if (!available) return new Response("embedding service unavailable", { status: 503 });
    const { input } = JSON.parse(String(init.body));
    return Response.json({ embeddings: input.map(() => Array(2048).fill(0.125)) });
  });
  try {
    await writeFile(join(root, "recovery.ts"), "export const recovery = 'repair the failed file';\n");
    await assert.rejects(index({ root }));
    available = true;
    const repaired = await index({ root });
    assert.equal(repaired.filesFailed, 0);
    const state = await status({ root });
    const scan = state.scan as { counts: Record<string, number> };
    assert.equal(scan.counts.filesIndexed, 1);
    assert.equal(scan.counts.filesFailed, 0);
    assert.equal(scan.counts.filesPending, 0);
  } finally {
    await closeService();
    await rm(root, { recursive: true, force: true });
  }
});

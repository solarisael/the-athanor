// Compares the source files that the map cites with the digests stored in its meta record.
// The map is hand-reviewed, so this module reports drift and changes nothing.

import { createHash } from "node:crypto";
import { join } from "node:path";

export type StaleState = "changed" | "missing";

export type MapMeta = { type: "meta"; revision: string; capturedAt: string; sourceFiles: Record<string, string> };

export async function readMap(mapPath: string): Promise<Record<string, any>[]> {
  const text = await Bun.file(mapPath).text();
  return text.trim().split("\n").map((line) => JSON.parse(line));
}

export async function staleFiles(repoRoot: string, meta: MapMeta): Promise<Map<string, StaleState>> {
  const stale = new Map<string, StaleState>();

  for (const [path, expected] of Object.entries(meta.sourceFiles)) {
    const file = Bun.file(join(repoRoot, path));

    if (!(await file.exists())) {
      stale.set(path, "missing");
      continue;
    }

    const actual = createHash("sha256").update(new Uint8Array(await file.arrayBuffer())).digest("hex");

    if (actual !== expected) stale.set(path, "changed");
  }

  return stale;
}

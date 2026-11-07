// Lists the anatomy records whose cited source changed after the snapshot.
// Exit code 1 means at least one cited file changed or disappeared.

import { join } from "node:path";
import { readMap, staleFiles, type MapMeta } from "./stale.ts";

const repoRoot = join(import.meta.dir, "..");
const records = await readMap(join(import.meta.dir, "anatomy.jsonl"));
const meta = records.find((record) => record.type === "meta") as MapMeta | undefined;

if (!meta) throw new Error("anatomy.jsonl has no meta record");

const changed = await staleFiles(repoRoot, meta);

console.log(`Snapshot ${meta.revision.slice(0, 8)} captured ${meta.capturedAt}`);
console.log(`${changed.size} of ${Object.keys(meta.sourceFiles).length} cited files differ.`);

if (changed.size === 0) process.exit(0);

for (const [path, state] of [...changed].sort()) {
  const cited = records.filter(
    (record) => record.type !== "meta" && record.sources?.some((source: { path: string }) => source.path === path),
  );
  const names = cited.map((record) => (record.type === "node" ? record.id : `${record.from} -${record.relation}-> ${record.to}`));

  console.log(`\n${state}: ${path} (${cited.length} records)`);
  for (const name of names) console.log(`  ${name}`);
}

process.exit(1);

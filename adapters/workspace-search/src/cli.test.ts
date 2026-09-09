import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { existsSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { test } from "node:test";

test("an invalid rebuild boolean cannot authorize an index write", () => {
  const root = mkdtempSync(join(tmpdir(), "athanor-rebuild-refusal-"));
  try {
    const result = spawnSync(process.execPath, [
      fileURLToPath(new URL("./cli.ts", import.meta.url)),
      "index", "--root", root, "--rebuild=no",
    ], { encoding: "utf8", timeout: 10_000 });
    assert.equal(result.status, 2, result.stderr);
    assert.equal(JSON.parse(result.stdout).error.code, "INVALID_ARGUMENTS");
    assert.equal(existsSync(join(root, ".zvec-grep")), false);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

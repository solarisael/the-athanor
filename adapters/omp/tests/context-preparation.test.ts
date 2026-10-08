import { expect, test } from "bun:test";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { readLegacyContextProposal } from "../house-proof/context.ts";

test("legacy adoption preserves valid bytes and never writes the source memo", () => {
  const root = mkdtempSync(path.join(tmpdir(), "athanor-context-adoption-"));
  const key = "room:session";
  const directory = path.join(root, ".omp", "runtime", "turn-additions");
  const file = path.join(directory, `${Bun.hash(key).toString(36)}.json`);
  mkdirSync(directory, { recursive: true });
  try {
    const source = JSON.stringify({ version: 1, turns: {
      visible: [{ customType: "solarisael-recall-context", content: "original\nbytes", timestamp: 1 }],
      hidden: [{ customType: "athanor-room-context", content: "old", timestamp: 2 }],
    } });
    writeFileSync(file, source);
    expect(readLegacyContextProposal(root, key, new Set(["visible"]))).toEqual({ status: "ready", memo: { version: 1, turns: [{
      turnId: "visible", blocks: [{ kind: "recall-context", content: "original\nbytes", timestamp: 1, details: null }],
    }] } });
    expect(readFileSync(file, "utf8")).toBe(source);
    const invalid = JSON.stringify({ version: 1, turns: {
      visible: [{ customType: "athanor-recall-context", content: String.fromCharCode(0xd800), timestamp: 1 }],
    } });
    writeFileSync(file, invalid);
    expect(readLegacyContextProposal(root, key, new Set(["visible"]))).toEqual({ status: "rejected", reason: "unpaired-surrogate" });
    expect(readFileSync(file, "utf8")).toBe(invalid);
  } finally {
    rmSync(root, { recursive: true });
  }
});

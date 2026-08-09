import { afterEach, describe, expect, test } from "bun:test";
import { mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";

import { clearVaultSearchCache, runVaultRecallQuery, searchVault } from "../index.ts";

const roots: string[] = [];

afterEach(async () => {
  clearVaultSearchCache();
  await Promise.all(roots.splice(0).map((root) => rm(root, { recursive: true, force: true })));
});

async function fixture() {
  const root = await mkdtemp(path.join(os.tmpdir(), "athanor-vault-search-"));
  roots.push(root);
  const room = path.join(root, "work-room");
  const alpha = path.join(root, "alpha-project");
  const beta = path.join(root, "beta-project");
  await Promise.all([mkdir(room), mkdir(alpha), mkdir(beta)]);
  await writeFile(path.join(room, ".solarisael-room.json"), JSON.stringify({
    version: 1,
    room: "work-room",
    vaultRoots: ["../alpha-project", "../beta-project"],
    vaultIgnore: ["private/**"],
  }));
  await writeFile(path.join(alpha, ".gitignore"), "ignored.md\n");
  await writeFile(path.join(alpha, "README.md"), [
    "---",
    "tags: [furnace, retrieval]",
    "---",
    "# Architecture",
    "The exact bridge identifier is HINGE-PROTOCOL-77.",
    "",
    "## Failure behavior",
    "Lexical recall remains available when embeddings disappear.",
  ].join("\n"));
  await writeFile(path.join(alpha, "ignored.md"), "HINGE-PROTOCOL-77 must never surface");
  await writeFile(path.join(alpha, ".env"), "HINGE-PROTOCOL-77=secret");
  await mkdir(path.join(alpha, "node_modules"));
  await writeFile(path.join(alpha, "node_modules", "noise.json"), JSON.stringify({ hinge: "HINGE-PROTOCOL-77" }));
  await mkdir(path.join(alpha, "private"));
  await writeFile(path.join(alpha, "private", "notes.md"), "HINGE-PROTOCOL-77 hidden");
  await writeFile(path.join(beta, "projects.json"), JSON.stringify({
    atlas: { owner: "Dino", sharedLibrary: "cross-project-capsule" },
    leo: { owner: "Leo", status: "evaluating" },
  }));
  await writeFile(path.join(beta, "events.jsonl"), [
    JSON.stringify({ type: "decision", project: "atlas", value: "cold models need attributed evidence" }),
    "{ malformed",
    JSON.stringify({ type: "receipt", project: "atlas", value: "vault-search-live" }),
  ].join("\n"));
  return { root, room, alpha, beta };
}

describe("Vault local retrieval", () => {
  test("searches configured Markdown roots with exact source and field attribution", async () => {
    const { room, alpha } = await fixture();
    const result = await searchVault(room, "HINGE-PROTOCOL-77");

    expect(result.ok).toBe(true);
    expect(result.source).toBe("vault-files");
    expect(result.found).toBe(true);
    expect(result.roots).toHaveLength(2);
    expect(result.retrievalCandidates[0]).toMatchObject({
      source_path: path.join(alpha, "README.md").replaceAll("\\", "/"),
      title: "README",
      heading_path: "Architecture",
      matched_terms: expect.arrayContaining(["hinge-protocol-77"]),
      reasons: expect.arrayContaining([expect.stringContaining("exact content fields: body")]),
    });
    expect(result.retrievalCandidates.every((candidate) => !candidate.source_path.includes("ignored.md"))).toBe(true);
    expect(result.retrievalCandidates.every((candidate) => !candidate.source_path.includes("node_modules"))).toBe(true);
    expect(result.retrievalCandidates.every((candidate) => !candidate.source_path.includes("private/"))).toBe(true);
  });

  test("parses JSON and JSONL records while reporting malformed lines", async () => {
    const { room, beta } = await fixture();
    const json = await searchVault(room, "cross-project-capsule Dino");
    expect(json.retrievalCandidates[0]).toMatchObject({
      source_path: path.join(beta, "projects.json").replaceAll("\\", "/"),
      heading_path: "/atlas",
    });
    expect(json.retrievalCandidates[0].reasons[0]).toContain("BM25F fields:");

    clearVaultSearchCache();
    const jsonl = await searchVault(room, "vault-search-live");
    expect(jsonl.retrievalCandidates[0]).toMatchObject({
      source_path: path.join(beta, "events.jsonl").replaceAll("\\", "/"),
      heading_path: expect.stringContaining("line:3"),
    });
    expect(jsonl.warnings).toEqual(expect.arrayContaining([expect.stringContaining("skipped 1 malformed JSONL record")]));
  });

  test("keeps room identity validation on the Vault recall contract", async () => {
    const { room } = await fixture();
    await expect(runVaultRecallQuery(room, "another-room", "hinge")).resolves.toMatchObject({
      ok: false,
      error: expect.stringContaining("room name/path mismatch"),
    });
    await expect(runVaultRecallQuery(room, "work-room", "hinge")).resolves.toMatchObject({
      ok: true,
      source: "vault-files",
      found: true,
    });
  });
});

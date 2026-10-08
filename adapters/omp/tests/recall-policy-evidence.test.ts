import { expect, test } from "bun:test";
import { randomUUID } from "node:crypto";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { activeProjectFromEvidence, markToolEvidence, toolEvidenceRevision } from "../house-proof/recall-policy.ts";

const session = () => ({ room: "kodo", spirit: "Kodo", session: randomUUID() });

test("only a bound touch changes the tool-evidence fact", () => {
  const binding = session();
  const sibling = session();
  expect(toolEvidenceRevision(binding)).toBe(0);
  markToolEvidence(binding);
  const first = toolEvidenceRevision(binding);
  expect(first).toBeGreaterThan(0);
  expect(toolEvidenceRevision(binding)).toBe(first);
  expect(toolEvidenceRevision(sibling)).toBe(0);
  markToolEvidence(binding);
  expect(toolEvidenceRevision(binding)).toBeGreaterThan(first);
  const unbound = { ...binding, session: "" };
  markToolEvidence(unbound);
  expect(toolEvidenceRevision(unbound)).toBe(0);
});

test("a worktree file identifies the observed project without granting a filesystem path", () => {
  const root = mkdtempSync(join(tmpdir(), "athanor-evidence-"));
  const repo = join(root, "Dragon-Repo");
  try {
    mkdirSync(repo);
    writeFileSync(join(repo, ".git"), "gitdir: elsewhere\n");
    const binding = session();
    markToolEvidence(binding, { paths: ["adapters/omp/index.ts"], cwd: repo });
    expect(activeProjectFromEvidence(binding)).toBe("dragon-repo");
  } finally {
    rmSync(root, { recursive: true });
  }
});

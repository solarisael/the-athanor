import { afterEach, describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { eligibilityContext } from "../hygiene.ts";

const roots: string[] = [];
afterEach(() => {
  for (const root of roots.splice(0)) rmSync(root, { recursive: true, force: true });
});

function project(root: string) {
  return { project: "eligibility-proof", root, source: "marker" as const, candidates: [root] };
}

describe("Striatum eligibility context", () => {
  test("derives language from the exact target extension", () => {
    const root = mkdtempSync(path.join(os.tmpdir(), "eligibility-"));
    roots.push(root);
    expect(eligibilityContext(project(root), path.join(root, "src", "main.rs"))).toEqual({
      languages: ["rust"], technologies: [],
    });
    expect(eligibilityContext(project(root), path.join(root, "query.sql"))).toEqual({
      languages: ["sql"], technologies: [],
    });
  });

  test("derives Godot and PostgreSQL only from project evidence", () => {
    const root = mkdtempSync(path.join(os.tmpdir(), "eligibility-"));
    roots.push(root);
    writeFileSync(path.join(root, "project.godot"), "[application]\nconfig/name=\"Proof\"\n");
    writeFileSync(path.join(root, "Cargo.toml"), "[dependencies]\nsqlx = { version = \"0.8\", features = [\"postgres\"] }\n");
    expect(eligibilityContext(project(root), path.join(root, "src", "main.rs"))).toEqual({
      languages: ["rust"], technologies: ["godot", "postgresql"],
    });
  });
});

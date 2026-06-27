import { test, expect } from "bun:test";
import { isScratchName, evaluateWrite, evaluateBashNudge } from "../hygiene.ts";
import os from "node:os";
import path from "node:path";

const VAULT = "C:\\Solarisael\\Obsidian\\obsidian";
const noMarker = (_dir: string) => false;

test("isScratchName flags throwaway shapes", () => {
  expect(isScratchName("kodo/.tmp_inventory.sh")).toBe(true);
  expect(isScratchName("D:/x/_zip_vault.ps1")).toBe(true);
  expect(isScratchName("_verify_fix.ps1")).toBe(true);
  expect(isScratchName("notes.tmp")).toBe(true);
  expect(isScratchName("foo_scratch.md")).toBe(true);
});

test("isScratchName leaves real files alone", () => {
  expect(isScratchName("kodo/memory/2026-06-27_session.md")).toBe(false);
  expect(isScratchName("memory.ts")).toBe(false);
  expect(isScratchName("index.json")).toBe(false);
  expect(isScratchName("zzzz_rubedo/_template.md")).toBe(false);
  expect(isScratchName("house/substrate/record_memory.py")).toBe(false);
});

test("evaluateWrite blocks scratch into the vault", () => {
  const d = evaluateWrite(VAULT + "\\kodo\\.tmp_inventory.sh");
  expect(d?.block).toBe(true);
  expect(d?.reason).toContain("tracked tree");
});

test("evaluateWrite allows a real memory file into the vault", () => {
  expect(evaluateWrite(VAULT + "\\kodo\\memory\\2026-06-27_x.md")).toBe(null);
});

test("evaluateWrite allows scratch OUTSIDE any tracked tree", () => {
  expect(evaluateWrite("C:\\some\\random\\.tmp_x.sh", noMarker)).toBe(null);
});

test("evaluateWrite allows scratch in the sanctioned .scratch dir", () => {
  expect(evaluateWrite(VAULT + "\\kodo\\work\\.scratch\\.tmp_x.sh")).toBe(null);
});

test("evaluateWrite blocks scratch in a .git/.omp/.opencode tree", () => {
  const yesMarker = (_dir: string) => true;
  const d = evaluateWrite("C:\\proj\\.tmp_x.sh", yesMarker);
  expect(d?.block).toBe(true);
});

test("evaluateBashNudge nudges bulk git add and forceful rm", () => {
  expect(evaluateBashNudge("git add -A")).toContain("git add");
  expect(evaluateBashNudge("git add .")).toContain("git add");
  expect(evaluateBashNudge("git add --all")).toContain("git add");
  expect(evaluateBashNudge("rm -rf /tmp/x")).toContain("rm");
  expect(evaluateBashNudge("rm -f foo")).toContain("rm");
});

test("evaluateBashNudge stays quiet on safe commands", () => {
  expect(evaluateBashNudge("git add src/foo.ts")).toBe(null);
  expect(evaluateBashNudge("git commit -m x")).toBe(null);
  expect(evaluateBashNudge("git status")).toBe(null);
  expect(evaluateBashNudge("ls -la")).toBe(null);
});

test("evaluateWrite does not treat the home dir itself as tracked", () => {
  const home = os.homedir();
  const probe = path.join(home, "Downloads", ".tmp_x.sh");
  expect(evaluateWrite(probe, (dir) => dir === path.resolve(home))).toBe(null);
});

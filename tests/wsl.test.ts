// Guards the Windows→WSL seam: path translation correctness and the
// runWsl outcome contract (timeout kill, stdout capture). Both callers
// (postgres source, coding-lessons) depend on exactly these behaviors.

import { describe, expect, test } from "bun:test";
import { runWsl, windowsPathToWsl } from "../wsl.ts";

describe("windowsPathToWsl", () => {
  test("translates a windows absolute path", () => {
    expect(windowsPathToWsl("C:\\foo\\bar")).toBe("/mnt/c/foo/bar");
  });

  test("lowercases the drive letter", () => {
    expect(windowsPathToWsl("D:\\Projects\\x")).toBe("/mnt/d/Projects/x");
  });

  test("accepts forward-slash windows paths", () => {
    expect(windowsPathToWsl("C:/already/forward")).toBe("/mnt/c/already/forward");
  });

  test("passes unix paths through untouched", () => {
    expect(windowsPathToWsl("/mnt/c/foo")).toBe("/mnt/c/foo");
  });

  test("normalizes backslashes on relative paths without inventing /mnt", () => {
    expect(windowsPathToWsl("relative\\path")).toBe("relative/path");
  });

  test("empty input falls through as empty string", () => {
    expect(windowsPathToWsl("")).toBe("");
  });
});

describe("runWsl (live wsl.exe)", () => {
  test("captures stdout and exit code 0", async () => {
    const outcome = await runWsl({
      argv: ["echo", "hello-from-wsl"],
      cwd: process.cwd(),
      timeoutMs: 15000,
    });
    expect(outcome.timedOut).toBe(false);
    expect(outcome.spawnError).toBeNull();
    expect(outcome.code).toBe(0);
    expect(outcome.stdout).toContain("hello-from-wsl");
  });

  test("kills a hung child and reports timedOut", async () => {
    const outcome = await runWsl({
      argv: ["sleep", "30"],
      cwd: process.cwd(),
      timeoutMs: 500,
    });
    expect(outcome.timedOut).toBe(true);
    expect(outcome.code).toBeNull();
  }, 10000);
});

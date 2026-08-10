import { describe, expect, test } from "bun:test";
import { mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import path from "node:path";
import { discoverRustExecutable, rustBinaryName, rustPlatform } from "../discovery.ts";

const roots: string[] = [];
async function temp() { const root = await mkdtemp(path.join(process.env.TEMP || ".", "rust-discovery-")); roots.push(root); return root; }

describe("Rust executable discovery", () => {
  test("platform names are stable", () => {
    expect(rustPlatform("win32", "x64")).toBe("windows-x64");
    expect(rustPlatform("linux", "x64")).toBe("linux-x64");
    expect(rustPlatform("linux", "arm64")).toBe("linux-arm64");
    expect(rustBinaryName("windows-x64")).toEndWith(".exe");
    expect(rustBinaryName("linux-x64")).not.toEndWith(".exe");
  });

  test("explicit path wins and invalid explicit paths fail", async () => {
    const root = await temp();
    const exact = path.join(root, "exact.exe");
    await writeFile(exact, "binary");
    expect(discoverRustExecutable({ env: { ATHANOR_SUBSTRATE_EXE: exact, ATHANOR_AUTO: "1" } })).toBe(exact);
    expect(() => discoverRustExecutable({ env: { ATHANOR_SUBSTRATE_EXE: path.join(root, "missing") } })).toThrow(/invalid executable/);
  });

  test("portable bundle precedes PATH only with auto opt-in", async () => {
    const root = await temp();
    const platform = rustPlatform("win32", "x64")!;
    const name = rustBinaryName(platform);
    const bundled = path.join(root, "bin", platform, name);
    const pathDir = path.join(root, "path");
    await mkdir(path.dirname(bundled), { recursive: true });
    await mkdir(pathDir, { recursive: true });
    await writeFile(bundled, "bundle");
    await writeFile(path.join(pathDir, name), "path");
    const env = { PATH: pathDir, ATHANOR_AUTO: "1" };
    expect(discoverRustExecutable({ env, platform: "win32", arch: "x64", moduleDir: root })).toBe(bundled);
    expect(discoverRustExecutable({ env: { PATH: pathDir }, platform: "win32", arch: "x64", moduleDir: root })).toBeNull();
    await rm(bundled);
    expect(discoverRustExecutable({ env, platform: "win32", arch: "x64", moduleDir: root })).toBe(path.join(pathDir, name));
  });
});

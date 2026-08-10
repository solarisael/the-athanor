import { afterEach, describe, expect, test } from "bun:test";
import { mkdir, mkdtemp, rm } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { comparablePath, layout } from "../install-layout.ts";

const adapterRoot = path.resolve(import.meta.dir, "..");
const verifier = path.join(adapterRoot, "verify-install.ts");
const temporary: string[] = [];

async function runVerifier(args: string[], env: Record<string, string | undefined>) {
  const child = Bun.spawn({
    cmd: [process.execPath, verifier, ...args],
    cwd: adapterRoot,
    env: { ...process.env, ...env },
    stdout: "pipe",
    stderr: "pipe",
  });
  const stdout = await new Response(child.stdout).text();
  return { exitCode: await child.exited, result: JSON.parse(stdout) };
}

afterEach(async () => {
  await Promise.all(temporary.splice(0).map((target) => rm(target, { recursive: true, force: true })));
});

describe("verify-install diagnostics", () => {
  test("reports missing config and invalid Rust selection with navigable redacted diagnostics", async () => {
    const root = await mkdtemp(path.join(os.tmpdir(), "omp-verify-diagnostics-"));
    temporary.push(root);
    const missingConfig = path.join(root, "missing-config.yml");
    const { exitCode, result } = await runVerifier(["--config", missingConfig], {
      ATHANOR_SUBSTRATE_ROOT: "",
      ATHANOR_SUBSTRATE_EXE: "postgres://user:password@private.example/rust?token=sensitive",
    });

    expect(exitCode).not.toBe(0);
    const rust = result.diagnostics.find((entry: any) => entry.observed.check === "Rust executable selection");
    const config = result.diagnostics.find((entry: any) => entry.observed.check === "OMP config");
    expect(rust).toMatchObject({
      category: "configuration",
      stage: "configuration_load",
      owner: { path: "verify-install.ts", symbol: "main" },
      execution: { request_dispatched: false, write_outcome: "not_started", retry: "after_change" },
    });
    expect(rust.next_checks).toHaveLength(2);
    expect(JSON.stringify(rust)).not.toContain("password");
    expect(JSON.stringify(rust)).not.toContain("sensitive");
    expect(config.targets).toContainEqual({ kind: "file", path: missingConfig });
  });

  test("reports missing compatibility schema with expected and observed values", async () => {
    const substrate = await mkdtemp(path.join(os.tmpdir(), "omp-verify-schema-"));
    temporary.push(substrate);
    const { result } = await runVerifier(["--profile", "akasha"], {
      ATHANOR_SUBSTRATE_ROOT: substrate,
    });

    const schema = result.diagnostics.find((entry: any) => entry.observed.check === "compatibility schema");
    expect(schema).toMatchObject({
      category: "protocol",
      expected: { check: "compatibility schema", ok: true },
      observed: { check: "compatibility schema", ok: false },
      targets: [{ kind: "file", path: path.join(substrate, "compatibility.json") }],
    });
  });

  test("accepts CRLF room identity headers on Windows", async () => {
    const root = await mkdtemp(path.join(os.tmpdir(), "omp-verify-crlf-room-"));
    temporary.push(root);
    const room = path.join(root, "old-room");
    await mkdir(room);
    await Bun.write(path.join(room, ".solarisael-room.json"), JSON.stringify({
      version: 1,
      room: "old-room",
      trueName: "Old Room",
      operator: "Sol",
    }));
    await Bun.write(
      path.join(room, "active_spirit.md"),
      "# Active Spirit: Old Room\r\nAgent: Old Room | Operator: Sol\r\n# SPIRIT: Old Room\r\n",
    );
    await Bun.write(path.join(room, "AGENTS.md"), "@active_spirit.md\r\n@room_summary.md\r\n");
    const { result } = await runVerifier(["--room", room, "--config", path.join(root, "missing.yml")], {
      ATHANOR_SUBSTRATE_ROOT: "",
      ATHANOR_SUBSTRATE_EXE: process.execPath,
    });

    for (const name of ["active spirit header", "agent/operator header", "spirit body"]) {
      expect(result.checks.find((check: any) => check.name === name)?.ok).toBe(true);
    }
  });

  test("no diagnostic names a 0.10.x topology variable", async () => {
    const root = await mkdtemp(path.join(os.tmpdir(), "omp-verify-legacy-names-"));
    temporary.push(root);
    const { result } = await runVerifier(["--config", path.join(root, "missing.yml")], {
      ATHANOR_SUBSTRATE_ROOT: "",
      ATHANOR_SUBSTRATE_EXE: process.execPath,
    });
    const serialized = JSON.stringify(result.diagnostics);
    for (const stale of ["SOLARISAEL_HOUSE_RUST", "SOLARISAEL_HOUSE_CORE", "SOLARISAEL_STATE_DIR", "SOLARISAEL_SUBSTRATE"]) {
      expect(serialized).not.toContain(stale);
    }
  });
});

describe("WSL and Windows path agreement", () => {
  // health.py runs through wsl.exe and answers with /mnt/c/... . Those are the
  // same directory as C:\... and must not read as drift; anything genuinely
  // different still must.
  const root = "C:\\Users\\Sol\\Athanor";
  const paths = layout(root);

  test("an equivalent WSL path compares equal to its Windows form", () => {
    expect(comparablePath("/mnt/c/Users/Sol/Athanor/state")).toBe(comparablePath(paths.state));
    expect(comparablePath("/mnt/c/Users/Sol/Athanor/state/substrate/backups")).toBe(comparablePath(paths.substrateBackups));
    expect(comparablePath("/mnt/c/Users/Sol/Athanor/the-athanor/adapters/omp/bin/windows-x64/athanor-substrate.exe"))
      .toBe(comparablePath(path.join(paths.adapter, "bin", "windows-x64", "athanor-substrate.exe")));
  });

  test("case, slash direction, and a trailing separator do not create drift", () => {
    expect(comparablePath("/MNT/C/Users/Sol/Athanor/state/")).toBe(comparablePath(paths.state));
    expect(comparablePath("C:/Users/Sol/Athanor/state")).toBe(comparablePath(paths.state));
    expect(comparablePath("c:\\users\\sol\\athanor\\state")).toBe(comparablePath(paths.state));
  });

  test("true drift still compares unequal", () => {
    // A different drive, a different root, and the development-checkout state
    // directory are all genuinely different places.
    expect(comparablePath("/mnt/d/Users/Sol/Athanor/state")).not.toBe(comparablePath(paths.state));
    expect(comparablePath("/mnt/c/Users/Sol/Elsewhere/state")).not.toBe(comparablePath(paths.state));
    expect(comparablePath("/mnt/c/Users/Sol/Athanor/the-athanor/state")).not.toBe(comparablePath(paths.state));
    expect(comparablePath("/mnt/c/Users/Sol/Athanor/state/substrate")).not.toBe(comparablePath(paths.substrateBackups));
  });

  test("an unresolvable report never silently matches", () => {
    // health.py reports null/absent when it cannot resolve a root; that must
    // not collapse onto a real path.
    expect(comparablePath("")).toBe("");
    expect(comparablePath("")).not.toBe(comparablePath(paths.state));
    expect(comparablePath("/mnt")).not.toBe(comparablePath(paths.state));
  });
});

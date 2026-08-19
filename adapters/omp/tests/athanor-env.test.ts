import { afterAll, describe, expect, test } from "bun:test";
import { mkdir, mkdtemp, realpath, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import {
  ADAPTER_ROOT,
  ATHANOR_ENV_FILE,
  ATHANOR_ROOT,
  ATHANOR_TOPOLOGY_KEYS,
  INSTALLED_COMPONENT,
  applyAthanorEnv,
  loadAthanorEnv,
  parseAthanorEnv,
} from "../athanor-root.ts";

const temporaryRoots: string[] = [];

async function makeRoot(): Promise<string> {
  // Windows hands out 8.3 short names for the temp directory (ADMINI~1), while
  // the module under test resolves real paths. Canonicalise here so comparisons
  // are about behaviour rather than about path spelling. This mirrors the
  // installer, which realpaths every value it writes.
  const root = await realpath(await mkdtemp(path.join(os.tmpdir(), "athanor-env-")));
  temporaryRoots.push(root);
  return root;
}

afterAll(async () => {
  for (const root of temporaryRoots) await rm(root, { recursive: true, force: true });
});

describe("structural roots", () => {
  test("source resolves the repository root from adapters/omp", () => {
    expect(INSTALLED_COMPONENT).toBe(false);
    expect(ADAPTER_ROOT.endsWith(path.join("adapters", "omp"))).toBe(true);
    expect(ATHANOR_ROOT).toBe(path.resolve(ADAPTER_ROOT, "..", ".."));
  });

  test("source topology configuration remains beside the repository root", () => {
    expect(ATHANOR_ENV_FILE).toBe(path.resolve(ATHANOR_ROOT, "..", "state", "athanor.env"));
    expect(path.basename(path.dirname(ATHANOR_ENV_FILE))).toBe("state");
  });
});

describe("athanor.env parsing", () => {
  test("reads plain assignments and ignores comments, blanks, and junk", () => {
    const parsed = parseAthanorEnv([
      "# installed by the Athanor installer",
      "",
      "ATHANOR_STATE_DIR=/install/state",
      "   ATHANOR_SUBSTRATE_ROOT = /install/the-athanor/substrate   ",
      "# ATHANOR_AUTO=1",
      "not an assignment",
      "=novalue",
    ].join("\n"));

    expect(parsed.get("ATHANOR_STATE_DIR")).toBe("/install/state");
    expect(parsed.get("ATHANOR_SUBSTRATE_ROOT")).toBe("/install/the-athanor/substrate");
    expect(parsed.has("ATHANOR_AUTO")).toBe(false);
    expect(parsed.has("")).toBe(false);
  });

  test("strips surrounding quotes but keeps inner content", () => {
    const parsed = parseAthanorEnv(`ATHANOR_STATE_DIR="C:\\Program Files\\Athanor\\state"`);
    expect(parsed.get("ATHANOR_STATE_DIR")).toBe("C:\\Program Files\\Athanor\\state");
  });
});

describe("athanor.env application", () => {
  test("sets canonical topology keys that are absent from the process", () => {
    const env: NodeJS.ProcessEnv = {};
    const applied = applyAthanorEnv([
      "ATHANOR_STATE_DIR=/install/state",
      "ATHANOR_SUBSTRATE_ROOT=/install/the-athanor/substrate",
      "ATHANOR_SUBSTRATE_EXE=/install/bin/athanor-substrate",
      "ATHANOR_AUTO=1",
    ].join("\n"), env);

    expect(applied.sort()).toEqual([...ATHANOR_TOPOLOGY_KEYS].sort());
    expect(env.ATHANOR_STATE_DIR).toBe("/install/state");
    expect(env.ATHANOR_SUBSTRATE_ROOT).toBe("/install/the-athanor/substrate");
    expect(env.ATHANOR_AUTO).toBe("1");
  });

  test("the real process environment wins over the file", () => {
    const env: NodeJS.ProcessEnv = { ATHANOR_STATE_DIR: "/operator/state" };
    const applied = applyAthanorEnv([
      "ATHANOR_STATE_DIR=/install/state",
      "ATHANOR_SUBSTRATE_ROOT=/install/the-athanor/substrate",
    ].join("\n"), env);

    expect(env.ATHANOR_STATE_DIR).toBe("/operator/state");
    expect(applied).toEqual(["ATHANOR_SUBSTRATE_ROOT"]);
  });

  test("an empty or whitespace-only process value is not a configured value", () => {
    // The opposite of the case above: blank must not shadow the file, because
    // nothing else in the Athanor treats an empty string as configuration.
    const env: NodeJS.ProcessEnv = { ATHANOR_STATE_DIR: "", ATHANOR_SUBSTRATE_ROOT: "   " };
    applyAthanorEnv([
      "ATHANOR_STATE_DIR=/install/state",
      "ATHANOR_SUBSTRATE_ROOT=/install/the-athanor/substrate",
    ].join("\n"), env);

    expect(env.ATHANOR_STATE_DIR).toBe("/install/state");
    expect(env.ATHANOR_SUBSTRATE_ROOT).toBe("/install/the-athanor/substrate");
  });

  test("an empty assignment in the file sets nothing", () => {
    const env: NodeJS.ProcessEnv = {};
    const applied = applyAthanorEnv("ATHANOR_STATE_DIR=\nATHANOR_AUTO=1", env);
    expect(applied).toEqual(["ATHANOR_AUTO"]);
    expect("ATHANOR_STATE_DIR" in env).toBe(false);
  });

  test("unknown keys and pre-cutover names are ignored, never aliased", () => {
    const env: NodeJS.ProcessEnv = {};
    const applied = applyAthanorEnv([
      "SOLARISAEL_STATE_DIR=/legacy/state",
      "SOLARISAEL_SUBSTRATE=/legacy/substrate",
      "SOLARISAEL_HOUSE_RUST=/legacy/rust.exe",
      "SOLARISAEL_HOUSE_CORE=/legacy/core",
      "PGPASSWORD=hunter2",
      "PATH=/attacker/bin",
    ].join("\n"), env);

    expect(applied).toEqual([]);
    // Nothing at all was set: not the legacy names, and not unrelated keys the
    // file has no business controlling.
    expect(Object.keys(env)).toEqual([]);
  });

  test("a missing file is silent and sets nothing", async () => {
    const root = await makeRoot();
    const env: NodeJS.ProcessEnv = {};
    expect(loadAthanorEnv(path.join(root, "absent.env"), env)).toEqual([]);
    expect(Object.keys(env)).toEqual([]);
  });

  test("an unreadable path is silent rather than fatal", async () => {
    // A directory where a file is expected must not make the adapter unloadable.
    const root = await makeRoot();
    const asDirectory = path.join(root, "athanor.env");
    await mkdir(asDirectory);
    const env: NodeJS.ProcessEnv = {};
    expect(loadAthanorEnv(asDirectory, env)).toEqual([]);
    expect(Object.keys(env)).toEqual([]);
  });

  test("loads a real file from disk", async () => {
    const root = await makeRoot();
    const file = path.join(root, "athanor.env");
    await writeFile(file, "# Athanor\nATHANOR_STATE_DIR=/install/state\n", "utf8");
    const env: NodeJS.ProcessEnv = {};
    expect(loadAthanorEnv(file, env)).toEqual(["ATHANOR_STATE_DIR"]);
    expect(env.ATHANOR_STATE_DIR).toBe("/install/state");
  });
});

describe("fresh process", () => {
  // The point of the whole feature: a brand-new process, with no topology in
  // its environment and no machine-level configuration, must pick the installed
  // values up from the file alone.
  //
  // To prove that honestly the module has to resolve its OWN location inside a
  // simulated installed component at
  // <program-root>/components/omp-adapter/versions/<releaseId>/.
  async function makeInstall(
    contents: string | null,
    componentsSegment = "components",
    componentSegment = "omp-adapter",
  ): Promise<string> {
    const installRoot = await makeRoot();
    const adapter = path.join(
      installRoot,
      componentsSegment,
      componentSegment,
      "versions",
      "0.9.3-example",
    );
    await mkdir(adapter, { recursive: true });
    await mkdir(path.join(installRoot, "state"), { recursive: true });
    await Bun.write(
      path.join(adapter, "athanor-root.ts"),
      await Bun.file(path.join(ADAPTER_ROOT, "athanor-root.ts")).text(),
    );
    if (contents !== null) {
      await writeFile(path.join(installRoot, "state", "athanor.env"), contents, "utf8");
    }
    return installRoot;
  }

  async function probeFreshProcess(
    installRoot: string,
    extraEnv: Record<string, string> = {},
    componentsSegment = "components",
    componentSegment = "omp-adapter",
  ) {
    const adapter = path.join(
      installRoot,
      componentsSegment,
      componentSegment,
      "versions",
      "0.9.3-example",
    );
    const probe = path.join(adapter, "probe.ts");
    await writeFile(
      probe,
      [
        `import { ATHANOR_ENV_APPLIED, ATHANOR_ENV_FILE, ATHANOR_ROOT } from "./athanor-root.ts";`,
        "console.log(JSON.stringify({",
        "  applied: ATHANOR_ENV_APPLIED,",
        "  file: ATHANOR_ENV_FILE,",
        "  root: ATHANOR_ROOT,",
        "  stateDir: process.env.ATHANOR_STATE_DIR ?? null,",
        "  substrateRoot: process.env.ATHANOR_SUBSTRATE_ROOT ?? null,",
        "  auto: process.env.ATHANOR_AUTO ?? null,",
        "}));",
      ].join("\n"),
      "utf8",
    );

    // A deliberately bare environment: nothing topology-related is inherited,
    // so the file is the only possible source of a value. Running from a
    // different cwd also proves the answer is structural, not cwd-relative.
    const child = Bun.spawnSync({
      cmd: [process.execPath, "run", probe],
      cwd: os.tmpdir(),
      env: { PATH: process.env.PATH ?? "", ...extraEnv },
      stdout: "pipe",
      stderr: "pipe",
    });
    const stdout = child.stdout.toString().trim();
    if (!stdout) throw new Error(`probe produced no output: ${child.stderr.toString()}`);
    return JSON.parse(stdout);
  }

  test("a fresh process with no topology in its environment reads the installed file", async () => {
    const install = await makeInstall([
      "# written by the Athanor installer",
      "ATHANOR_STATE_DIR=/install/state",
      "ATHANOR_SUBSTRATE_ROOT=/install/the-athanor/substrate",
    ].join("\n"));

    const result = await probeFreshProcess(install);

    expect(result.file).toBe(path.join(install, "state", "athanor.env"));
    expect(result.root).toBe(install);
    expect(result.applied.sort()).toEqual(["ATHANOR_STATE_DIR", "ATHANOR_SUBSTRATE_ROOT"]);
    expect(result.stateDir).toBe("/install/state");
    expect(result.substrateRoot).toBe("/install/the-athanor/substrate");
  });

  test("Windows casing variants remain installed in a fresh process", async () => {
    if (process.platform !== "win32") return;
    const componentsSegment = "CoMpOnEnTs";
    const componentSegment = "OMP-AdApTeR";
    const install = await makeInstall(
      "ATHANOR_STATE_DIR=C:/installed/state",
      componentsSegment,
      componentSegment,
    );
    const result = await probeFreshProcess(
      install,
      {},
      componentsSegment,
      componentSegment,
    );

    expect(result.root).toBe(install);
    expect(result.file).toBe(path.join(install, "state", "athanor.env"));
    expect(result.applied).toEqual(["ATHANOR_STATE_DIR"]);
    expect(result.stateDir).toBe("C:/installed/state");
  });

  test("a fresh process without the file gets nothing, not a guess", async () => {
    // The alternative to the case above. Absent configuration must stay absent
    // — this is a Vault-from-source run, and inventing a state root here is
    // exactly the silent fallback the cutover removed.
    const install = await makeInstall(null);
    const result = await probeFreshProcess(install);

    expect(result.applied).toEqual([]);
    expect(result.stateDir).toBe(null);
    expect(result.substrateRoot).toBe(null);
  });

  test("a fresh process still lets an explicit environment value win", async () => {
    const install = await makeInstall("ATHANOR_STATE_DIR=/install/state");
    const result = await probeFreshProcess(install, { ATHANOR_STATE_DIR: "/from/environment" });

    expect(result.stateDir).toBe("/from/environment");
    expect(result.applied).not.toContain("ATHANOR_STATE_DIR");
  });

  test("a fresh process ignores pre-cutover names in the installed file", async () => {
    const install = await makeInstall([
      "SOLARISAEL_STATE_DIR=/legacy/state",
      "SOLARISAEL_SUBSTRATE=/legacy/substrate",
    ].join("\n"));

    const result = await probeFreshProcess(install);

    expect(result.applied).toEqual([]);
    expect(result.stateDir).toBe(null);
    expect(result.substrateRoot).toBe(null);
  });
});

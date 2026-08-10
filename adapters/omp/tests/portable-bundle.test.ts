import { createHash } from "node:crypto";
import { afterAll, afterEach, beforeAll, describe, expect, test } from "bun:test";
import { mkdir, mkdtemp, readFile, readdir, rm, stat, writeFile } from "node:fs/promises";
import { spawn } from "node:child_process";
import os from "node:os";
import path from "node:path";
import { pathToFileURL } from "node:url";
import {
  ADAPTER_RELATIVE,
  DEVELOPMENT_MARKER_ENTRIES,
  DEVELOPMENT_MARKER_PREFIXES,
  PRODUCT_DIRECTORY,
  REQUIRED_ENTRIES,
  SUBSTRATE_RELATIVE,
  archiveName,
  archivePlatform,
  substrateBinaryRelative,
  type Profile,
} from "../install-layout.ts";

type CommandResult = { stdout: string; stderr: string; exitCode: number };

const tempRoots: string[] = [];
const adapterRoot = path.resolve(import.meta.dir, "..");
const productRoot = path.resolve(adapterRoot, "..", "..");
const constantsModule = pathToFileURL(path.join(adapterRoot, "solarisael-house-proof", "constants.ts")).href;
const rootModule = pathToFileURL(path.join(adapterRoot, "athanor-root.ts")).href;
const hygieneModule = pathToFileURL(path.join(adapterRoot, "hygiene.ts")).href;
const portableBuilder = path.join(adapterRoot, "build-portable.ts");
const verifier = path.join(adapterRoot, "verify-install.ts");
const platform = archivePlatform();
if (!platform) throw new Error("portable bundle tests require a supported archive platform");

function runAllowFailure(command: string, args: string[], cwd: string, env: NodeJS.ProcessEnv): Promise<CommandResult> {
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, { cwd, env, windowsHide: true });
    let stdout = "";
    let stderr = "";
    child.stdout.on("data", (chunk) => stdout += chunk);
    child.stderr.on("data", (chunk) => stderr += chunk);
    let exitCode = -1;
    child.on("error", reject);
    // `exit` can precede the final chunks. This helper parses the builder's
    // stdout JSON and greps tar listings, so a short read is indistinguishable
    // from a genuinely malformed bundle.
    child.on("exit", (value) => { exitCode = value ?? -1; });
    child.on("close", () => resolve({ stdout, stderr, exitCode }));
  });
}

async function run(command: string, args: string[], cwd: string, env: NodeJS.ProcessEnv): Promise<CommandResult> {
  const result = await runAllowFailure(command, args, cwd, env);
  if (result.exitCode !== 0) {
    throw new Error(`${command} exited with code ${result.exitCode}\n${result.stdout}${result.stderr}`);
  }
  return result;
}

/**
 * A shell with no Athanor topology at all. Every test that cares about topology
 * sets exactly the variables it means to exercise, so an ambient SOLARISAEL_* or
 * ATHANOR_* in the developer's own shell cannot make a check pass by accident.
 */
function isolatedEnv(home: string, overrides: Record<string, string | undefined> = {}): NodeJS.ProcessEnv {
  const drive = path.parse(home).root.replace(/[\\/]+$/, "");
  const relativeHome = path.relative(path.parse(home).root, home).replaceAll("/", "\\");
  return {
    PATH: process.env.PATH,
    PATHEXT: process.env.PATHEXT,
    SystemRoot: process.env.SystemRoot,
    WINDIR: process.env.WINDIR,
    ComSpec: process.env.ComSpec,
    HOME: home,
    USERPROFILE: home,
    HOMEDRIVE: drive,
    HOMEPATH: relativeHome ? `\\${relativeHome}` : "\\",
    TEMP: path.join(home, "temp"),
    TMP: path.join(home, "temp"),
    SOLARISAEL_TEST_NATIVE_PYTHON: process.env.SOLARISAEL_TEST_NATIVE_PYTHON,
    ATHANOR_STATE_DIR: home,
    ATHANOR_SUBSTRATE_ROOT: path.join(home, "missing-substrate"),
    ATHANOR_SUBSTRATE_EXE: process.execPath,
    ...overrides,
  };
}

async function makeTempRoot(prefix: string) {
  const root = await mkdtemp(path.join(os.tmpdir(), prefix));
  tempRoots.push(root);
  return root;
}

function normalizedEntries(listing: string): string[] {
  return listing
    .split(/\r?\n/)
    .map((entry) => entry.replaceAll("\\", "/").replace(/^\.\/+/, "").replace(/\/+$/, ""))
    .filter(Boolean);
}

async function runConstantsProbe(env: NodeJS.ProcessEnv) {
  const root = await makeTempRoot("omp-portable-probe-");
  const probe = path.join(root, "probe.ts");
  await writeFile(probe, `import path from "node:path";
import { ADAPTER_ROOT, ATHANOR_ROOT } from ${JSON.stringify(rootModule)};
import { OBSIDIAN_ROOT } from ${JSON.stringify(constantsModule)};
import { isInTrackedTree } from ${JSON.stringify(hygieneModule)};

console.log(JSON.stringify({
  adapterRoot: ADAPTER_ROOT,
  productRoot: ATHANOR_ROOT,
  obsidianRoot: OBSIDIAN_ROOT,
  vaultPathIsTracked: isInTrackedTree(path.join(OBSIDIAN_ROOT, "example-room", "note.tmp"), () => false),
}));
`, "utf8");
  const result = await run(process.execPath, [probe], root, env);
  return JSON.parse(result.stdout) as {
    adapterRoot: string;
    productRoot: string;
    obsidianRoot: string;
    vaultPathIsTracked: boolean;
  };
}

afterEach(async () => {
  await Promise.all(tempRoots.splice(0).map((root) => rm(root, { recursive: true, force: true })));
});

describe("portable bundle path contract", () => {
  test("resolves the product root structurally, never through an override or a sibling checkout", async () => {
    const builderSource = await readFile(portableBuilder, "utf8");
    const rootSource = await readFile(path.join(adapterRoot, "athanor-root.ts"), "utf8");

    expect(builderSource).toContain('import { ADAPTER_ROOT, ATHANOR_ROOT } from "./athanor-root.ts"');
    expect(builderSource).not.toContain("projectsRoot");
    expect(rootSource).toContain('path.resolve(ADAPTER_ROOT, "..", "..")');
    // The 0.10.x core override is gone. If it comes back, the installed adapter
    // can be pointed at a core that is not the one it shipped with.
    expect(rootSource).not.toContain("SOLARISAEL_HOUSE_CORE");
    expect(builderSource).not.toContain("SOLARISAEL_HOUSE_CORE");
  });

  test("ignores a stale SOLARISAEL_HOUSE_CORE in the environment", async () => {
    const root = await makeTempRoot("omp-portable-core-override-");
    const home = path.join(root, "home");
    const alternateCore = path.join(root, "alternate-core");
    await mkdir(home, { recursive: true });
    await mkdir(alternateCore, { recursive: true });

    const result = await runConstantsProbe(isolatedEnv(home, { SOLARISAEL_HOUSE_CORE: alternateCore }));

    expect(result.productRoot).not.toBe(path.resolve(alternateCore));
    expect(result.productRoot).toBe(productRoot);
    expect(result.adapterRoot).toBe(adapterRoot);
  });

  test("uses an isolated SOLARISAEL_VAULT_ROOT for both constants and hygiene", async () => {
    const root = await makeTempRoot("omp-portable-vault-override-");
    const home = path.join(root, "home");
    const vault = path.join(root, "portable-vault");
    await mkdir(home, { recursive: true });
    await mkdir(vault, { recursive: true });

    const result = await runConstantsProbe(isolatedEnv(home, { SOLARISAEL_VAULT_ROOT: vault }));

    expect(result.obsidianRoot).toBe(path.resolve(vault));
    expect(result.vaultPathIsTracked).toBe(true);
  });

  test("falls back to the isolated home directory's Solarisael vault", async () => {
    const root = await makeTempRoot("omp-portable-vault-fallback-");
    const home = path.join(root, "home");
    await mkdir(home, { recursive: true });

    const result = await runConstantsProbe(isolatedEnv(home));

    expect(result.obsidianRoot).toBe(path.join(home, "Solarisael"));
    expect(result.vaultPathIsTracked).toBe(true);
  });
});

describe("portable bundle profiles", () => {
  let home = "";
  let distribution = "";
  let version = "";
  let substrateStub = "";
  const built = new Map<Profile, { archive: string; entries: string[]; extracted: string }>();
  // Deliberately NOT registered with the module-global `tempRoots`: that array
  // is spliced empty by afterEach, so the first test in this block would delete
  // the archives every later test reads. This root is torn down in afterAll.
  let sharedRoot = "";

  beforeAll(async () => {
    const root = await mkdtemp(path.join(os.tmpdir(), "omp-portable-profiles-"));
    sharedRoot = root;
    home = path.join(root, "home");
    distribution = path.join(root, "dist");
    substrateStub = path.join(root, "athanor-substrate.exe");
    await mkdir(path.join(home, "temp"), { recursive: true });
    await mkdir(distribution, { recursive: true });
    // A stand-in for the cargo build output; the builder only copies and hashes it.
    await writeFile(substrateStub, "#!/bin/sh\nexit 0\n");
    version = JSON.parse(await readFile(path.join(adapterRoot, "package.json"), "utf8")).version;

    const build = await run(process.execPath, [portableBuilder, "--profile", "all", "--out-dir", distribution], adapterRoot,
      isolatedEnv(home, { ATHANOR_SUBSTRATE_EXE: substrateStub }));
    const result = JSON.parse(build.stdout) as {
      version: string;
      platform: string;
      archives: Array<{ profile: Profile; path: string; sha256: string; size: number }>;
    };
    expect(result.version).toBe(version);
    expect(result.platform).toBe(platform);
    expect(result.archives.map((entry) => entry.profile).sort()).toEqual(["akasha", "vault"]);

    for (const archive of result.archives) {
      expect(path.basename(archive.path)).toBe(archiveName(version, platform!, archive.profile));
      const listing = await run("tar", ["-tf", archive.path], root, isolatedEnv(home));
      const extracted = path.join(root, `extracted-${archive.profile}`);
      await mkdir(extracted, { recursive: true });
      await run("tar", ["-xf", archive.path, "-C", extracted], root, isolatedEnv(home));
      built.set(archive.profile, { archive: archive.path, entries: normalizedEntries(listing.stdout), extracted });
    }
  }, 120_000);

  afterAll(async () => {
    if (sharedRoot) await rm(sharedRoot, { recursive: true, force: true });
  });

  test("stdout carries only the result JSON, so a release job can parse it", async () => {
    const source = await readFile(portableBuilder, "utf8");
    // `stdio: "inherit"` would put bun-compile chatter on stdout ahead of the JSON.
    expect(source).not.toContain('stdio: "inherit"');
    expect(source).toContain('stdio: ["ignore", 2, 2]');
  });

  test("both archives carry one product directory and the onboarding surface", () => {
    for (const [profile, bundle] of built) {
      const topLevel = new Set(bundle.entries.map((entry) => entry.split("/")[0] as string));
      expect(topLevel, profile).toContain(PRODUCT_DIRECTORY);
      for (const stale of ["solarisael-house", "solarisael-house-omp", "solarisael-house-substrate"]) {
        expect([...topLevel], profile).not.toContain(stale);
      }
      expect(bundle.entries, profile).toEqual(expect.arrayContaining([
        "README.md",
        "INSTALL.md",
        "USAGE.md",
        "IDENTITY_GUIDE.md",
        "LICENSE",
        "NOTICE",
        "SETUP.txt",
        ...REQUIRED_ENTRIES,
      ]));
      // Rooms and state are created by the installer, never shipped.
      expect([...topLevel], profile).not.toContain("rooms");
      expect([...topLevel], profile).not.toContain("state");
    }
  });

  test("the Vault archive carries no substrate binary and no substrate operations", () => {
    const vault = built.get("vault")!;
    expect(vault.entries.some((entry) => entry.startsWith(`${SUBSTRATE_RELATIVE}/`))).toBe(false);
    expect(vault.entries.some((entry) => entry.startsWith(`${ADAPTER_RELATIVE}/bin/`))).toBe(false);
    expect(vault.entries).not.toContain(`${ADAPTER_RELATIVE}/rust-manifest.json`);
    expect(vault.entries.some((entry) => /athanor-substrate(\.exe)?$/.test(entry))).toBe(false);
  });

  test("neither archive can be mistaken for a development checkout", () => {
    // The substrate's Python and bash state resolvers treat Cargo.toml plus
    // crates/ as the development-checkout signal. If either shipped, an
    // installed tree would resolve state through the development fallback
    // instead of <install-root>/state.
    for (const [profile, bundle] of built) {
      for (const entry of DEVELOPMENT_MARKER_ENTRIES) {
        expect(bundle.entries, `${profile}/${entry}`).not.toContain(entry);
      }
      for (const prefix of DEVELOPMENT_MARKER_PREFIXES) {
        expect(bundle.entries.some((entry) => entry.startsWith(prefix)), `${profile}/${prefix}`).toBe(false);
      }
      expect(bundle.entries.some((entry) => /(^|\/)Cargo\.(toml|lock)$/.test(entry)), profile).toBe(false);
    }
  });

  test("the AKASHA archive carries the substrate operations and the platform binary", () => {
    const akasha = built.get("akasha")!;
    expect(akasha.entries).toEqual(expect.arrayContaining([
      `${SUBSTRATE_RELATIVE}/health.py`,
      `${SUBSTRATE_RELATIVE}/compatibility.json`,
      `${SUBSTRATE_RELATIVE}/state_paths.py`,
      `${SUBSTRATE_RELATIVE}/backup.sh`,
      `${ADAPTER_RELATIVE}/rust-manifest.json`,
      `${ADAPTER_RELATIVE}/${substrateBinaryRelative(platform!)}`,
    ]));
    // No credentials, caches, or dumps ride along.
    expect(akasha.entries.some((entry) => entry.endsWith("/.env"))).toBe(false);
    expect(akasha.entries.some((entry) => entry.includes("__pycache__"))).toBe(false);
    expect(akasha.entries.some((entry) => entry.includes("/.venv/"))).toBe(false);
    expect(akasha.entries.some((entry) => entry.endsWith(".dump"))).toBe(false);
  });

  test("each package manifest names its own profile, platform, and every staged artifact", async () => {
    for (const [profile, bundle] of built) {
      const manifest = JSON.parse(await readFile(path.join(bundle.extracted, ADAPTER_RELATIVE, "package-manifest.json"), "utf8"));
      expect(manifest, profile).toMatchObject({
        schemaVersion: 3,
        productVersion: version,
        profile,
        platform,
        productDirectory: PRODUCT_DIRECTORY,
        adapterDirectory: ADAPTER_RELATIVE,
        installer: process.platform === "win32" ? "install.exe" : "install",
        updater: process.platform === "win32" ? "update.exe" : "update",
        supportedHarnesses: ["omp"],
        requiredSchemaVersion: 14,
      });
      expect(manifest.substrateBinary, profile).toBe(profile === "akasha" ? substrateBinaryRelative(platform!) : null);

      const artifactPaths = manifest.artifacts.map((artifact: { path: string }) => artifact.path);
      expect(new Set(artifactPaths).size, profile).toBe(artifactPaths.length);
      expect(artifactPaths, profile).toEqual([...artifactPaths].sort((left: string, right: string) => left.localeCompare(right)));

      const stagedFiles: string[] = [];
      const collect = async (directory: string): Promise<void> => {
        for (const entry of await readdir(directory, { withFileTypes: true })) {
          const file = path.join(directory, entry.name);
          if (entry.isDirectory()) await collect(file);
          else if (entry.isFile()) stagedFiles.push(path.relative(bundle.extracted, file).replaceAll("\\", "/"));
        }
      };
      await collect(bundle.extracted);
      expect(artifactPaths, profile).toEqual(
        stagedFiles
          .filter((file) => file !== `${ADAPTER_RELATIVE}/package-manifest.json`)
          .sort((left, right) => left.localeCompare(right)),
      );
      for (const artifact of manifest.artifacts) {
        const bytes = await readFile(path.join(bundle.extracted, artifact.path));
        expect(bytes.byteLength, artifact.path).toBe(artifact.size);
        expect(createHash("sha256").update(bytes).digest("hex"), artifact.path).toBe(artifact.sha256);
      }
    }
  }, 60_000);

  test("building only one profile emits only that archive", async () => {
    const root = await makeTempRoot("omp-portable-single-");
    const singleHome = path.join(root, "home");
    const out = path.join(root, "dist");
    await mkdir(path.join(singleHome, "temp"), { recursive: true });
    // No substrate variable at all: the Vault build must not need one.
    const build = await run(process.execPath, [portableBuilder, "--profile", "vault", "--out-dir", out], adapterRoot, isolatedEnv(singleHome));
    const result = JSON.parse(build.stdout);
    expect(result.archives).toHaveLength(1);
    expect(result.archives[0].profile).toBe("vault");
    expect((await readdir(out)).filter((entry) => entry.endsWith(".zip"))).toEqual([archiveName(version, platform!, "vault")]);
  }, 60_000);

  test("an AKASHA build without a substrate executable fails loudly", async () => {
    const root = await makeTempRoot("omp-portable-no-substrate-");
    const bareHome = path.join(root, "home");
    await mkdir(path.join(bareHome, "temp"), { recursive: true });
    const result = await runAllowFailure(
      process.execPath,
      [portableBuilder, "--profile", "akasha", "--out-dir", path.join(root, "dist")],
      adapterRoot,
      isolatedEnv(bareHome, { ATHANOR_SUBSTRATE_EXE: path.join(bareHome, "missing-substrate.exe") }),
    );
    expect(result.exitCode).not.toBe(0);
    expect(result.stderr).toContain("ATHANOR_SUBSTRATE_EXE");
  }, 60_000);

  test("an unknown profile is refused", async () => {
    const root = await makeTempRoot("omp-portable-bad-profile-");
    const badHome = path.join(root, "home");
    await mkdir(path.join(badHome, "temp"), { recursive: true });
    const result = await runAllowFailure(process.execPath, [portableBuilder, "--profile", "full", "--out-dir", path.join(root, "dist")], adapterRoot, isolatedEnv(badHome));
    expect(result.exitCode).not.toBe(0);
    expect(result.stderr).toContain("--profile must be vault, akasha, or all");
  }, 30_000);

  test("the build never touches the operator's OMP config", async () => {
    const root = await makeTempRoot("omp-portable-config-");
    const configHome = path.join(root, "home");
    const configPath = path.join(configHome, ".omp", "agent", "config.yml");
    const before = "extensions:\n  - keep-this-config-untouched.ts\n";
    await mkdir(path.dirname(configPath), { recursive: true });
    await mkdir(path.join(configHome, "temp"), { recursive: true });
    await writeFile(configPath, before, "utf8");
    await run(process.execPath, [portableBuilder, "--profile", "vault", "--out-dir", path.join(root, "dist")], adapterRoot, isolatedEnv(configHome));
    expect(await readFile(configPath, "utf8")).toBe(before);
  }, 60_000);
});

describe("installed verification", () => {
  async function room(root: string, trueName = "Example Room", operator = "Ada Lovelace") {
    const directory = path.join(root, "example");
    await mkdir(directory, { recursive: true });
    await writeFile(path.join(directory, ".solarisael-room.json"), `${JSON.stringify({ version: 1, room: "example", trueName, operator }, null, 2)}\n`, "utf8");
    await writeFile(path.join(directory, "active_spirit.md"), [
      `# Active Spirit: ${trueName}`,
      `Agent: ${trueName} | Operator: ${operator}`,
      `Embodied: ${trueName} | Conjured: none | Summoned: none`,
      "",
      `# SPIRIT: ${trueName}`,
      "A complete portable room identity.",
      "",
    ].join("\n"), "utf8");
    await writeFile(path.join(directory, "AGENTS.md"), "Read @active_spirit.md and @room_summary.md before acting.\n", "utf8");
    return directory;
  }

  test("verifies a complete Vault room with no substrate configured", async () => {
    const root = await makeTempRoot("omp-verify-vault-");
    const home = path.join(root, "home");
    await mkdir(path.join(home, "temp"), { recursive: true });
    const directory = await room(root);
    const configPath = path.join(home, ".omp", "agent", "config.yml");
    await mkdir(path.dirname(configPath), { recursive: true });
    await writeFile(configPath, `extensions:\n  - ${path.join(adapterRoot, "index.ts")}\n  - ${path.join(adapterRoot, "hygiene.ts")}\n`, "utf8");

    const result = await runAllowFailure(process.execPath, [verifier, "--room", directory, "--config", configPath, "--profile", "vault"], adapterRoot, isolatedEnv(home));
    expect(result.exitCode, result.stdout).toBe(0);
    expect(JSON.parse(result.stdout)).toMatchObject({
      ok: true,
      mode: "Vault",
      profile: "vault",
      staticOk: true,
      roomPath: directory,
      substrateRoot: null,
      runtimeHealth: { state: "not-configured", ok: null },
    });
  }, 30_000);

  test("a missing host context entrypoint fails the room", async () => {
    const root = await makeTempRoot("omp-verify-room-");
    const home = path.join(root, "home");
    await mkdir(path.join(home, "temp"), { recursive: true });
    const directory = await room(root);
    await rm(path.join(directory, "AGENTS.md"), { force: true });
    const configPath = path.join(home, ".omp", "agent", "config.yml");
    await mkdir(path.dirname(configPath), { recursive: true });
    await writeFile(configPath, `extensions:\n  - ${path.join(adapterRoot, "index.ts")}\n  - ${path.join(adapterRoot, "hygiene.ts")}\n`, "utf8");

    const result = await runAllowFailure(process.execPath, [verifier, "--room", directory, "--config", configPath, "--profile", "vault"], adapterRoot, isolatedEnv(home));
    expect(result.exitCode).not.toBe(0);
    expect(JSON.parse(result.stdout)).toMatchObject({
      ok: false,
      checks: expect.arrayContaining([expect.objectContaining({ name: "host context entrypoint", ok: false })]),
    });
  }, 30_000);

  test("a stale 0.10.x extension path in the config fails the wiring checks", async () => {
    const root = await makeTempRoot("omp-verify-config-");
    const home = path.join(root, "home");
    await mkdir(path.join(home, "temp"), { recursive: true });
    const directory = await room(root);
    const configPath = path.join(home, ".omp", "agent", "config.yml");
    await mkdir(path.dirname(configPath), { recursive: true });
    await writeFile(configPath, "extensions:\n  - C:/old/solarisael-house-omp/index.ts\n  - C:/old/solarisael-house-omp/hygiene.ts\n", "utf8");

    const result = await runAllowFailure(process.execPath, [verifier, "--room", directory, "--config", configPath, "--profile", "vault"], adapterRoot, isolatedEnv(home));
    expect(result.exitCode).not.toBe(0);
    const parsed = JSON.parse(result.stdout);
    expect(parsed.checks).toContainEqual(expect.objectContaining({ name: "OMP entrypoint configured", ok: false }));
    expect(parsed.checks).toContainEqual(expect.objectContaining({ name: "OMP hygiene configured", ok: false }));
  }, 30_000);

  test("AKASHA compatibility, health, and schema are all load-bearing", async () => {
    const root = await makeTempRoot("omp-verify-akasha-");
    const home = path.join(root, "home");
    await mkdir(path.join(home, "temp"), { recursive: true });
    const directory = await room(root);
    const configPath = path.join(home, ".omp", "agent", "config.yml");
    await mkdir(path.dirname(configPath), { recursive: true });
    await writeFile(configPath, `extensions:\n  - ${path.join(adapterRoot, "index.ts")}\n  - ${path.join(adapterRoot, "hygiene.ts")}\n`, "utf8");

    const substrate = path.join(root, "substrate");
    const contract = path.join(substrate, "compatibility.json");
    const healthScript = path.join(substrate, "health.py");
    await mkdir(substrate, { recursive: true });
    const env = isolatedEnv(home, { ATHANOR_SUBSTRATE_ROOT: substrate });
    const args = [verifier, "--room", directory, "--config", configPath, "--profile", "akasha"];

    await writeFile(contract, `${JSON.stringify({ format: 1, substrateApi: 1, coreApi: 1, adapterApi: 1, schemaVersion: 13 }, null, 2)}\n`, "utf8");
    const missingHealth = await runAllowFailure(process.execPath, args, adapterRoot, env);
    expect(missingHealth.exitCode).not.toBe(0);
    expect(JSON.parse(missingHealth.stdout)).toMatchObject({
      mode: "degraded",
      profile: "akasha",
      runtimeHealth: { ok: false, state: "unhealthy" },
    });

    await writeFile(healthScript, `print(${JSON.stringify(JSON.stringify({ ok: true, mode: "full", substrateApi: 1, degradedReasons: [] }))})\n`, "utf8");
    const healthy = await runAllowFailure(process.execPath, args, adapterRoot, env);
    expect(healthy.exitCode, healthy.stdout).toBe(0);
    expect(JSON.parse(healthy.stdout)).toMatchObject({
      ok: true,
      mode: "AKASHA",
      profile: "akasha",
      substrateRoot: substrate,
      compatibility: { ok: true },
      runtimeHealth: { ok: true, state: "healthy", verdict: { mode: "full" } },
    });

    for (const api of ["substrateApi", "coreApi", "adapterApi"]) {
      await writeFile(contract, `${JSON.stringify({ format: 1, substrateApi: 1, coreApi: 1, adapterApi: 1, schemaVersion: 13, [api]: 2 }, null, 2)}\n`, "utf8");
      const mismatch = await runAllowFailure(process.execPath, args, adapterRoot, env);
      expect(mismatch.exitCode).not.toBe(0);
      const parsed = JSON.parse(mismatch.stdout);
      expect(parsed.mode).toBe("degraded");
      expect(parsed.checks).toContainEqual(expect.objectContaining({
        name: `${api === "substrateApi" ? "substrate" : api === "coreApi" ? "core" : "adapter"} API compatibility`,
        ok: false,
      }));
    }

    await rm(contract, { force: true });
    const missingContract = await runAllowFailure(process.execPath, args, adapterRoot, env);
    expect(missingContract.exitCode).not.toBe(0);
    expect(JSON.parse(missingContract.stdout).mode).toBe("degraded");

    await writeFile(contract, `${JSON.stringify({ format: 1, substrateApi: 1, coreApi: 1, adapterApi: 1, schemaVersion: 13 }, null, 2)}\n`, "utf8");
    await writeFile(healthScript, `print(${JSON.stringify(JSON.stringify({ ok: false, mode: "degraded", substrateApi: 1, degradedReasons: ["database unavailable"] }))})\n`, "utf8");
    const unhealthy = await runAllowFailure(process.execPath, args, adapterRoot, env);
    expect(unhealthy.exitCode).not.toBe(0);
    expect(JSON.parse(unhealthy.stdout)).toMatchObject({
      mode: "degraded",
      runtimeHealth: { ok: false, state: "unhealthy", verdict: { degradedReasons: ["database unavailable"] } },
    });
  }, 120_000);

  test("a Vault verification never resolves a substrate, even with one configured", async () => {
    const root = await makeTempRoot("omp-verify-vault-strict-");
    const home = path.join(root, "home");
    await mkdir(path.join(home, "temp"), { recursive: true });
    const directory = await room(root);
    const configPath = path.join(home, ".omp", "agent", "config.yml");
    await mkdir(path.dirname(configPath), { recursive: true });
    await writeFile(configPath, `extensions:\n  - ${path.join(adapterRoot, "index.ts")}\n  - ${path.join(adapterRoot, "hygiene.ts")}\n`, "utf8");

    const substrate = path.join(root, "substrate");
    await mkdir(substrate, { recursive: true });
    await writeFile(path.join(substrate, "compatibility.json"), JSON.stringify({ format: 1, substrateApi: 1, coreApi: 1, adapterApi: 1, schemaVersion: 13 }));

    const result = await runAllowFailure(
      process.execPath,
      [verifier, "--room", directory, "--config", configPath, "--profile", "vault"],
      adapterRoot,
      isolatedEnv(home, { ATHANOR_SUBSTRATE_ROOT: substrate }),
    );
    expect(result.exitCode, result.stdout).toBe(0);
    const parsed = JSON.parse(result.stdout);
    expect(parsed.mode).toBe("Vault");
    expect(parsed.substrateRoot).toBeNull();
    expect(parsed.runtimeHealth.state).toBe("not-configured");
  }, 30_000);

  test("an unknown --profile is refused outright", async () => {
    const root = await makeTempRoot("omp-verify-bad-profile-");
    const home = path.join(root, "home");
    await mkdir(path.join(home, "temp"), { recursive: true });
    const result = await runAllowFailure(process.execPath, [verifier, "--profile", "full"], adapterRoot, isolatedEnv(home));
    expect(result.exitCode).not.toBe(0);
    expect(result.stderr).toContain("--profile must be vault or akasha");
  }, 30_000);
});

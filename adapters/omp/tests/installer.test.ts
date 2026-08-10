import { afterEach, describe, expect, test } from "bun:test";
import { mkdir, mkdtemp, readFile, readdir, realpath, rm, stat, utimes, writeFile } from "node:fs/promises";
import { spawn } from "node:child_process";
import os from "node:os";
import path from "node:path";
import {
  ADAPTER_RELATIVE,
  PRODUCT_DIRECTORY,
  SUBSTRATE_RELATIVE,
  archivePlatform,
  substrateBinaryRelative,
  type Profile,
} from "../install-layout.ts";
import { preserveLegacy } from "../installer.ts";
import type { LegacyDetection } from "../install-legacy.ts";

const rootDir = path.resolve(import.meta.dir, "..");
const installer = path.join(rootDir, "installer.ts");
const platform = archivePlatform();
if (!platform) throw new Error("installer tests require a supported archive platform");
const roots: string[] = [];

const run = (args: string[]) => new Promise<{ code: number; stdout: string; stderr: string }>((resolve, reject) => {
  const child = spawn(process.execPath, [installer, ...args], { windowsHide: true });
  let stdout = "", stderr = "";
  child.stdout.on("data", (chunk) => stdout += chunk);
  child.stderr.on("data", (chunk) => stderr += chunk);
  let code = -1;
  child.on("error", reject);
  // Same reason as installer.ts exec(): `exit` can precede the final stdout and
  // stderr chunks. Most assertions here read stderr, so a short read under
  // full-suite load would produce a spurious red that looks like a real defect.
  child.on("exit", (value) => { code = value ?? -1; });
  child.on("close", () => resolve({ code, stdout, stderr }));
});

/** A verifier that honours --require-manifest and echoes the requested profile. */
const STUB_VERIFIER = `if (!process.argv.includes("--require-manifest")) process.exit(42);
const index = process.argv.indexOf("--profile");
const profile = index >= 0 ? process.argv[index + 1] : "vault";
console.log(JSON.stringify({ ok: true, mode: profile === "akasha" ? "AKASHA" : "Vault", profile }));
`;

type FixtureOptions = {
  profile?: Profile;
  verifier?: string;
  packageManifest?: boolean;
  /** Drop the substrate assets from an AKASHA bundle, or add them to a Vault one. */
  substrateAssets?: boolean;
  /** Let the manifest claim a profile the archive shape does not match. */
  manifestProfile?: Profile;
  /** Let the manifest claim a platform this host is not. */
  manifestPlatform?: string;
  /** Plant a file that makes the tree look like a development checkout. */
  developmentMarker?: string;
};

async function fixture(options: FixtureOptions = {}) {
  const profile = options.profile ?? "vault";
  const substrate = options.substrateAssets ?? (profile === "akasha");
  const root = await mkdtemp(path.join(os.tmpdir(), "athanor-installer-test-"));
  roots.push(root);
  const tree = path.join(root, "tree");
  const adapter = path.join(tree, ADAPTER_RELATIVE);
  await mkdir(path.join(adapter, "starter-room", "example"), { recursive: true });

  await writeFile(path.join(tree, PRODUCT_DIRECTORY, "index.ts"), "export const CORE_API_VERSION=1;\n");
  await writeFile(path.join(tree, PRODUCT_DIRECTORY, "package.json"), '{"name":"the-athanor","version":"0.11.0"}\n');
  await writeFile(path.join(adapter, "index.ts"), "export const ADAPTER_API_VERSION=1;\n");
  for (const name of ["hygiene.ts", "discovery.ts", "harnesses.ts", "athanor-root.ts", "install-layout.ts"]) {
    await writeFile(path.join(adapter, name), "export {};\n");
  }
  await writeFile(path.join(adapter, "verify-install.ts"), options.verifier ?? STUB_VERIFIER);
  if (options.packageManifest !== false) {
    await writeFile(path.join(adapter, "package-manifest.json"), JSON.stringify({
      schemaVersion: 3,
      productVersion: "0.11.0",
      profile: options.manifestProfile ?? profile,
      platform: options.manifestPlatform ?? platform,
      installer: "install.exe",
      updater: "update.exe",
    }));
  }
  await writeFile(path.join(adapter, "starter-room", "example", ".solarisael-room.json"), JSON.stringify({ version: 1, room: "example", trueName: "Example", operator: "Example" }));
  await writeFile(path.join(adapter, "starter-room", "example", "active_spirit.md"), "# Active Spirit: Example\nAgent: Example | Operator: Example\n# SPIRIT: Example\n");
  await writeFile(path.join(adapter, "starter-room", "example", "AGENTS.md"), "@active_spirit.md\n@room_summary.md\nSTARTER GUIDANCE\n");
  await writeFile(path.join(adapter, "starter-room", "example", "room_summary.md"), "STARTER SUMMARY\n");

  if (substrate) {
    await mkdir(path.join(tree, SUBSTRATE_RELATIVE), { recursive: true });
    for (const name of ["health.py", "compatibility.json", "state_paths.py", "backup.sh"]) {
      await writeFile(path.join(tree, SUBSTRATE_RELATIVE, name), "# substrate operation\n");
    }
    await writeFile(path.join(adapter, "rust-manifest.json"), JSON.stringify({ artifacts: [] }));
    const binary = path.join(adapter, substrateBinaryRelative(platform!));
    await mkdir(path.dirname(binary), { recursive: true });
    await writeFile(binary, "substrate binary\n");
  }

  if (options.developmentMarker) {
    const marker = path.join(tree, PRODUCT_DIRECTORY, options.developmentMarker);
    await mkdir(path.dirname(marker), { recursive: true });
    await writeFile(marker, "[workspace]\n");
  }

  const zip = path.join(root, "bundle.zip");
  const tar = spawn("tar", ["-a", "-c", "-f", zip, "-C", tree, "."], { windowsHide: true });
  await new Promise<void>((resolve, reject) => {
    tar.on("error", reject);
    tar.on("close", (code) => code === 0 ? resolve() : reject(new Error("tar failed")));
  });
  return { root, zip };
}

/** A 0.10.x install root: three sibling product directories plus operator data. */
async function legacyTree(root: string, options: { backup?: "fresh" | "stale" | "corrupt" | "none" } = {}) {
  const target = path.join(root, "legacy");
  for (const directory of ["solarisael-house", "solarisael-house-omp", "solarisael-house-substrate/backups", "rooms/old-room"]) {
    await mkdir(path.join(target, directory), { recursive: true });
  }
  await writeFile(path.join(target, "solarisael-house", "index.ts"), "export const CORE_API_VERSION=1;\n");
  await writeFile(path.join(target, "solarisael-house-omp", "package-manifest.json"), JSON.stringify({ schemaVersion: 2, productVersion: "0.10.1" }));
  await writeFile(path.join(target, "solarisael-house-substrate", ".env"), "PGUSER=solarisael\nPGDATABASE=solarisael_memory\n");
  await writeFile(path.join(target, "rooms", "old-room", ".solarisael-room.json"), JSON.stringify({ version: 1, room: "old-room", trueName: "Old Room", operator: "Sol" }));
  await writeFile(path.join(target, "rooms", "old-room", "active_spirit.md"), "# Active Spirit: Old Room\nAgent: Old Room | Operator: Sol\n# SPIRIT: Old Room\nLEGACY IDENTITY\n");
  await writeFile(path.join(target, "rooms", "old-room", "AGENTS.md"), "@active_spirit.md\n@room_summary.md\nLEGACY GUIDANCE\n");
  await writeFile(path.join(target, "rooms", "old-room", "room_summary.md"), "LEGACY SUMMARY\n");
  await writeFile(path.join(target, "rooms", "old-room", "notes.md"), "irreplaceable operator notes");

  const backup = options.backup ?? "none";
  if (backup !== "none") {
    const dump = path.join(target, "solarisael-house-substrate", "backups", "solarisael_memory.dump");
    await writeFile(dump, backup === "corrupt"
      ? Buffer.from("this is not a pg_dump archive")
      : Buffer.concat([Buffer.from("PGDMP"), Buffer.alloc(64, 1)]));
    if (backup === "stale") {
      const when = new Date(Date.now() - 8 * 24 * 3_600_000);
      await utimes(dump, when, when);
    }
  }
  return target;
}

afterEach(async () => {
  await Promise.all(roots.splice(0).map((root) => rm(root, { recursive: true, force: true })));
});

describe.serial("installer bundle validation", () => {
  test("rejects an unsafe room without touching the target", async () => {
    const { root, zip } = await fixture();
    const target = path.join(root, "new target");
    const result = await run(["--bundle", zip, "--target", target, "--room", "../escape", "--mode", "vault", "--dry-run"]);
    expect(result.code).not.toBe(0);
    expect(await stat(target).catch(() => null)).toBeNull();
  });

  test("dry-run validates a bundle while leaving the target absent", async () => {
    const { root, zip } = await fixture();
    const target = path.join(root, "target with spaces");
    const result = await run(["--bundle", zip, "--target", target, "--room", "demo-room", "--mode", "vault", "--dry-run"]);
    expect(result.code, JSON.stringify(result)).toBe(0);
    expect(JSON.parse(result.stdout)).toMatchObject({ dryRun: true, profile: "vault" });
    expect(await stat(target).catch(() => null)).toBeNull();
  });

  test("requires the unified package manifest by its canonical path", async () => {
    const { root, zip } = await fixture({ packageManifest: false });
    const target = path.join(root, "target");
    const result = await run(["--bundle", zip, "--target", target, "--room", "demo", "--mode", "vault", "--dry-run"]);
    expect(result.code).not.toBe(0);
    expect(result.stderr).toContain(`bundle missing required file: ${ADAPTER_RELATIVE}/package-manifest.json`);
    expect(await stat(target).catch(() => null)).toBeNull();
  });

  test("refuses a bundle that still carries 0.10.x product directories", async () => {
    const root = await mkdtemp(path.join(os.tmpdir(), "athanor-installer-stale-"));
    roots.push(root);
    const tree = path.join(root, "tree");
    await mkdir(path.join(tree, "solarisael-house-omp"), { recursive: true });
    await writeFile(path.join(tree, "solarisael-house-omp", "index.ts"), "export {};\n");
    const zip = path.join(root, "stale.zip");
    await new Promise<void>((resolve, reject) => {
      const tar = spawn("tar", ["-a", "-c", "-f", zip, "-C", tree, "."], { windowsHide: true });
      tar.on("error", reject);
      tar.on("close", (code) => code === 0 ? resolve() : reject(new Error("tar failed")));
    });
    const result = await run(["--bundle", zip, "--target", path.join(root, "target"), "--room", "demo", "--mode", "vault", "--dry-run"]);
    expect(result.code).not.toBe(0);
    expect(result.stderr).toMatch(/missing required file|0\.10\.x product directory/);
  });

  test("a Vault bundle cannot be installed as AKASHA", async () => {
    const { root, zip } = await fixture({ profile: "vault" });
    const result = await run(["--bundle", zip, "--target", path.join(root, "target"), "--room", "demo", "--mode", "akasha", "--dry-run"]);
    expect(result.code).not.toBe(0);
    expect(result.stderr).toContain(`AKASHA bundle missing required file: ${SUBSTRATE_RELATIVE}/health.py`);
  });

  test("an AKASHA bundle cannot be installed as Vault", async () => {
    const { root, zip } = await fixture({ profile: "akasha" });
    const result = await run(["--bundle", zip, "--target", path.join(root, "target"), "--room", "demo", "--mode", "vault", "--dry-run"]);
    expect(result.code).not.toBe(0);
    expect(result.stderr).toContain("Vault bundle must not contain");
  });

  test("a Vault bundle carrying substrate assets is refused", async () => {
    const { root, zip } = await fixture({ profile: "vault", substrateAssets: true });
    const result = await run(["--bundle", zip, "--target", path.join(root, "target"), "--room", "demo", "--mode", "vault", "--dry-run"]);
    expect(result.code).not.toBe(0);
    expect(result.stderr).toContain("Vault bundle must not contain");
  });

  test("an AKASHA-shaped archive whose manifest claims vault is refused", async () => {
    // The archive passes the entry-shape gate for akasha, so only the manifest
    // cross-check can catch it. Without that check the tree would install under
    // the wrong profile and the verifier would be handed the wrong mode.
    const { root, zip } = await fixture({ profile: "akasha", manifestProfile: "vault" });
    const result = await run(["--bundle", zip, "--target", path.join(root, "target"), "--room", "demo", "--mode", "akasha", "--dry-run"]);
    expect(result.code).not.toBe(0);
    expect(result.stderr).toContain("bundle profile is vault, not akasha");
  });

  test("a manifest built for another platform is refused", async () => {
    const other = platform === "windows-x64" ? "linux-x64" : "windows-x64";
    const { root, zip } = await fixture({ manifestPlatform: other });
    const result = await run(["--bundle", zip, "--target", path.join(root, "target"), "--room", "demo", "--mode", "vault", "--dry-run"]);
    expect(result.code).not.toBe(0);
    expect(result.stderr).toContain(`bundle platform is ${other}, not ${platform}`);
  });

  for (const marker of ["Cargo.toml", "Cargo.lock", "crates/house-core/Cargo.toml", "target/release/athanor-substrate.exe"] as const) {
    test(`a bundle carrying the development marker ${marker} is refused`, async () => {
      // Cargo.toml + crates/ is exactly how the substrate's Python and bash
      // resolvers recognise a development checkout. Shipping either would make
      // an installed tree resolve state through the development fallback and
      // write runtime state where nobody looks.
      const { root, zip } = await fixture({ developmentMarker: marker });
      const result = await run(["--bundle", zip, "--target", path.join(root, "target"), "--room", "demo", "--mode", "vault", "--dry-run"]);
      expect(result.code).not.toBe(0);
      expect(result.stderr).toContain("development-checkout marker");
    });
  }
});

describe.serial("installer public CLI", () => {
  test("accepts the supported harness once and reports the catalog entry", async () => {
    const { root, zip } = await fixture();
    const result = await run(["--bundle", zip, "--target", path.join(root, "target"), "--room", "demo", "--mode", "vault", "--harness", "omp", "--harness", "omp", "--dry-run"]);
    expect(result.code).toBe(0);
    expect(JSON.parse(result.stdout).harnesses).toEqual(["omp"]);
  });

  test("rejects an unsupported harness before mutating the target", async () => {
    const { root, zip } = await fixture();
    const target = path.join(root, "target");
    const result = await run(["--bundle", zip, "--target", target, "--room", "demo", "--mode", "vault", "--harness", "unknown", "--dry-run"]);
    expect(result.code).not.toBe(0);
    expect(result.stderr).toContain("unsupported harness: unknown");
    expect(await stat(target).catch(() => null)).toBeNull();
  });

  test("rejects an unknown --mode outright", async () => {
    const { root, zip } = await fixture();
    const target = path.join(root, "target");
    const result = await run(["--bundle", zip, "--target", target, "--room", "demo", "--mode", "everything", "--dry-run"]);
    expect(result.code).not.toBe(0);
    expect(result.stderr).toContain("vault|akasha");
    expect(await stat(target).catch(() => null)).toBeNull();
  });

  test("the shipped harness descriptor names the unified adapter directory", async () => {
    const result = await run(["--list-harnesses"]);
    expect(result.code).toBe(0);
    const [omp] = JSON.parse(result.stdout).harnesses;
    // Public through --list-harnesses: a stale value teaches callers a
    // directory the installer no longer creates.
    expect(omp.adapterDirectory).toBe(ADAPTER_RELATIVE);
    expect(omp.adapterDirectory).not.toBe("solarisael-house-omp");
    expect(omp.entrypoints).toEqual(["index.ts", "hygiene.ts"]);
  });

  for (const legacyMode of ["base", "full"] as const) {
    test(`--mode ${legacyMode} is refused by the public CLI`, async () => {
      const { root, zip } = await fixture();
      const result = await run(["--bundle", zip, "--target", path.join(root, "target"), "--room", "demo", "--mode", legacyMode, "--dry-run"]);
      expect(result.code).not.toBe(0);
      expect(result.stderr).toContain("is a 0.10.x token");
      expect(result.stderr).toContain(legacyMode === "base" ? "--mode vault" : "--mode akasha");
    });
  }

  test("--migrate-legacy without any 0.10.x signal is refused", async () => {
    const { root, zip } = await fixture();
    const result = await run([
      "--bundle", zip,
      "--target", path.join(root, "target"),
      "--room", "demo",
      "--mode", "vault",
      "--config", path.join(root, "clean-config.yml"),
      "--migrate-legacy",
      "--dry-run",
    ]);
    expect(result.code).not.toBe(0);
    expect(result.stderr).toContain("no 0.10.x layout");
  });
});

/** A fresh custom-format dump the operator took before installing. */
async function freshDump(root: string, name = "solarisael_memory.dump") {
  const dump = path.join(root, name);
  await writeFile(dump, Buffer.concat([Buffer.from("PGDMP"), Buffer.alloc(64, 5)]));
  return dump;
}

describe.serial("installer substrate credentials", () => {
  test("fresh AKASHA refuses without a backup, before it even looks at credentials", async () => {
    const { root, zip } = await fixture({ profile: "akasha" });
    const target = path.join(root, "install");
    const result = await run(["--bundle", zip, "--target", target, "--room", "demo", "--mode", "akasha", "--config", path.join(root, "config.yml"), "--dry-run"]);
    expect(result.code).not.toBe(0);
    // "Fresh" is not a loophole: an absent backups directory is the absence of
    // safety, so the gate fires before anything else can succeed.
    expect(result.stderr).toContain("AKASHA activation refused");
    // A first-time operator is not migrating anything and must not be told so.
    expect(result.stderr).not.toContain("migration refused");
    expect(result.stderr).toContain("no PostgreSQL backup was found");
    expect(result.stderr).toContain("--backup PATH");
    expect(await stat(target).catch(() => null)).toBeNull();
  });

  test("fresh AKASHA refuses without --env-file once a backup is supplied", async () => {
    const { root, zip } = await fixture({ profile: "akasha" });
    const target = path.join(root, "install");
    const dump = await freshDump(root);
    const result = await run(["--bundle", zip, "--target", target, "--room", "demo", "--mode", "akasha", "--config", path.join(root, "config.yml"), "--backup", dump, "--dry-run"]);
    expect(result.code).not.toBe(0);
    expect(result.stderr).toContain("--env-file");
    expect(await stat(target).catch(() => null)).toBeNull();
  });

  test("fresh AKASHA seeds both the dotenv and the backup, and preserves them through activation", async () => {
    const { root, zip } = await fixture({ profile: "akasha" });
    const target = path.join(root, "install");
    const source = path.join(root, "operator.env");
    await writeFile(source, "PGUSER=solarisael\nPGDATABASE=solarisael_memory\nPGPASSWORD=secret\n");
    const dump = await freshDump(root);

    const result = await run(["--bundle", zip, "--target", target, "--room", "demo", "--mode", "akasha", "--config", path.join(root, "config.yml"), "--env-file", source, "--backup", dump]);
    expect(result.code, JSON.stringify(result)).toBe(0);
    // Both land at canonical locations, so nothing downstream needs a second path.
    expect(await readFile(path.join(target, "state", "substrate", ".env"), "utf8")).toContain("PGDATABASE=solarisael_memory");
    // Normalised into backup.sh's rotation family so later pruning manages it.
    const receipt = JSON.parse(result.stdout).backup;
    expect(receipt.source).toBe(dump);
    expect(path.basename(receipt.destination)).toMatch(/^solarisael_memory_\d{4}-\d{2}-\d{2}_\d{6}\.dump$/);
    expect(receipt.destination).toBe(path.join(target, "state", "substrate", "backups", path.basename(receipt.destination)));
    expect((await readFile(receipt.destination)).subarray(0, 5).toString()).toBe("PGDMP");
    // The operator's own file keeps its name and stays where they left it.
    expect(await readFile(source, "utf8")).toContain("PGPASSWORD=secret");
    expect(path.basename(dump)).toBe("solarisael_memory.dump");
    expect(await stat(dump)).toBeDefined();
  });

  test("fresh AKASHA refuses a stale or non-PGDMP backup", async () => {
    const { root, zip } = await fixture({ profile: "akasha" });
    const notADump = path.join(root, "notes.dump");
    await writeFile(notADump, "this is not a pg_dump archive");
    const corrupt = await run(["--bundle", zip, "--target", path.join(root, "install"), "--room", "demo", "--mode", "akasha", "--config", path.join(root, "config.yml"), "--backup", notADump, "--dry-run"]);
    expect(corrupt.code).not.toBe(0);
    expect(corrupt.stderr).toContain("missing PGDMP header");

    const stale = await freshDump(root, "old.dump");
    const when = new Date(Date.now() - 8 * 24 * 3_600_000);
    await utimes(stale, when, when);
    const aged = await run(["--bundle", zip, "--target", path.join(root, "install"), "--room", "demo", "--mode", "akasha", "--config", path.join(root, "config.yml"), "--backup", stale, "--dry-run"]);
    expect(aged.code).not.toBe(0);
    expect(aged.stderr).toContain("freshness window");
  });

  test("Vault refuses both --env-file and --backup outright", async () => {
    const { root, zip } = await fixture({ profile: "vault" });
    const source = path.join(root, "operator.env");
    await writeFile(source, "PGUSER=solarisael\n");
    const env = await run(["--bundle", zip, "--target", path.join(root, "install"), "--room", "demo", "--mode", "vault", "--config", path.join(root, "config.yml"), "--env-file", source, "--dry-run"]);
    expect(env.code).not.toBe(0);
    expect(env.stderr).toContain("only accepted for --mode akasha");

    const dump = await freshDump(root);
    const backup = await run(["--bundle", zip, "--target", path.join(root, "install"), "--room", "demo", "--mode", "vault", "--config", path.join(root, "config.yml"), "--backup", dump, "--dry-run"]);
    expect(backup.code).not.toBe(0);
    expect(backup.stderr).toContain("--backup is only accepted for --mode akasha");

    const window = await run(["--bundle", zip, "--target", path.join(root, "install"), "--room", "demo", "--mode", "vault", "--config", path.join(root, "config.yml"), "--backup-max-age-hours", "48", "--dry-run"]);
    expect(window.code).not.toBe(0);
    expect(window.stderr).toContain("--backup-max-age-hours is only accepted for --mode akasha");
  });

  test("--env-file must be absolute and must exist", async () => {
    const { root, zip } = await fixture({ profile: "akasha" });
    const dump = await freshDump(root);
    const base = ["--bundle", zip, "--target", path.join(root, "install"), "--room", "demo", "--mode", "akasha", "--config", path.join(root, "config.yml"), "--backup", dump, "--dry-run"];
    const relative = await run([...base, "--env-file", "relative.env"]);
    expect(relative.code).not.toBe(0);
    expect(relative.stderr).toContain("--env-file must be an absolute path");

    const missing = await run([...base, "--env-file", path.join(root, "absent.env")]);
    expect(missing.code).not.toBe(0);
    expect(missing.stderr).toContain("must be a readable regular file");
  });

  test("legacy migration keeps the detected 0.10.x dotenv and refuses --env-file", async () => {
    const { root, zip } = await fixture({ profile: "akasha" });
    const target = await legacyTree(root, { backup: "fresh" });
    const source = path.join(root, "operator.env");
    await writeFile(source, "PGUSER=other\n");
    const refused = await run(["--bundle", zip, "--target", target, "--room", "old-room", "--mode", "akasha", "--config", path.join(root, "config.yml"), "--migrate-legacy", "--env-file", source]);
    expect(refused.code).not.toBe(0);
    expect(refused.stderr).toContain("not accepted with --migrate-legacy");

    const migrated = await run(["--bundle", zip, "--target", target, "--room", "old-room", "--mode", "akasha", "--config", path.join(root, "config.yml"), "--migrate-legacy"]);
    expect(migrated.code, JSON.stringify(migrated)).toBe(0);
    expect(await readFile(path.join(target, "state", "substrate", ".env"), "utf8")).toContain("PGDATABASE=solarisael_memory");
  });
});

describe.serial("installer install shape", () => {
  test("lays down one product directory beside rooms and state, with no siblings", async () => {
    const { root, zip } = await fixture();
    const target = path.join(root, "install");
    const config = path.join(root, "config.yml");
    const result = await run(["--bundle", zip, "--target", target, "--room", "demo", "--mode", "vault", "--config", config]);
    expect(result.code, JSON.stringify(result)).toBe(0);

    const entries = (await readdir(target, { withFileTypes: true })).filter((entry) => entry.isDirectory()).map((entry) => entry.name);
    expect(entries.sort()).toEqual([PRODUCT_DIRECTORY, "rooms", "state"].sort());
    for (const stale of ["solarisael-house", "solarisael-house-omp", "solarisael-house-substrate"]) {
      expect(entries).not.toContain(stale);
    }
    expect(await stat(path.join(target, ADAPTER_RELATIVE, "index.ts"))).toBeDefined();
    expect(await stat(path.join(target, "rooms", "demo", ".solarisael-room.json"))).toBeDefined();
  });

  test("emits only the canonical topology variables", async () => {
    const { root, zip } = await fixture();
    const target = path.join(root, "install");
    const result = await run(["--bundle", zip, "--target", target, "--room", "demo", "--mode", "vault", "--config", path.join(root, "config.yml")]);
    expect(result.code, JSON.stringify(result)).toBe(0);

    const environment = JSON.parse(result.stdout).environment as Record<string, string>;
    expect(Object.keys(environment)).toEqual(["ATHANOR_STATE_DIR"]);
    expect(environment.ATHANOR_STATE_DIR).toBe(path.join(await realpath(target), "state"));

    // The env file is mutable state, never inside the immutable product tree.
    expect(await stat(path.join(target, "athanor.env")).catch(() => null)).toBeNull();
    expect(await stat(path.join(target, PRODUCT_DIRECTORY, "athanor.env")).catch(() => null)).toBeNull();
    const file = await readFile(path.join(target, "state", "athanor.env"), "utf8");
    expect(file).toContain(`ATHANOR_STATE_DIR=${path.join(await realpath(target), "state")}`);
    expect(file).not.toContain("ATHANOR_SUBSTRATE_ROOT");
    expect(file).not.toContain("ATHANOR_SUBSTRATE_EXE");
    expect(file).not.toContain("ATHANOR_AUTO");
    for (const stale of ["SOLARISAEL_HOUSE_CORE", "SOLARISAEL_HOUSE_RUST", "SOLARISAEL_SUBSTRATE", "SOLARISAEL_STATE_DIR"]) {
      expect(file).not.toContain(stale);
    }
  });

  test("writes only canonical extension paths and strips every stale generation", async () => {
    const { root, zip } = await fixture();
    const target = path.join(root, "install");
    const config = path.join(root, "omp-config.yml");
    await writeFile(config, [
      "model: user-choice",
      "extensions:",
      "  - keep-extension.ts",
      "  - C:/old/install/solarisael-house-omp/index.ts",
      "  - C:/old/install/solarisael-house-omp/hygiene.ts",
      "  - D:/elsewhere/the-athanor/adapters/omp/index.ts",
      "",
    ].join("\n"));

    const result = await run(["--bundle", zip, "--target", target, "--room", "demo", "--mode", "vault", "--config", config]);
    expect(result.code, JSON.stringify(result)).toBe(0);

    const written = await readFile(config, "utf8");
    const canonical = path.join(await realpath(target), ADAPTER_RELATIVE).replaceAll("\\", "/");
    expect(written).toContain("model: user-choice");
    expect(written).toContain("keep-extension.ts");
    expect(written).toContain(`${canonical}/index.ts`);
    expect(written).toContain(`${canonical}/hygiene.ts`);
    expect(written).not.toContain("solarisael-house-omp");
    expect(written).not.toContain("D:/elsewhere");
    const extensionLines = written.split(/\r?\n/).filter((line) => /(?:index|hygiene)\.ts\s*$/.test(line));
    expect(extensionLines).toHaveLength(2);
  });

  test("existing target refuses without force", async () => {
    const { root, zip } = await fixture();
    const target = path.join(root, "existing");
    await mkdir(target);
    await writeFile(path.join(target, "keep"), "yes");
    const result = await run(["--bundle", zip, "--target", target, "--room", "demo", "--mode", "vault"]);
    expect(result.code).not.toBe(0);
    expect(await readFile(path.join(target, "keep"), "utf8")).toBe("yes");
  });

  test("force preserves rooms and user files while fully replacing the product tree", async () => {
    const { root, zip } = await fixture();
    const target = path.join(root, "existing");
    await mkdir(path.join(target, "rooms", "old-room"), { recursive: true });
    await mkdir(path.join(target, ADAPTER_RELATIVE), { recursive: true });
    await writeFile(path.join(target, "rooms", "old-room", "notes.md"), "keep me");
    await writeFile(path.join(target, "operator.txt"), "keep user file");
    await writeFile(path.join(target, ADAPTER_RELATIVE, "removed-in-this-release.ts"), "export {};\n");

    const result = await run(["--bundle", zip, "--target", target, "--room", "demo", "--mode", "vault", "--force", "--config", path.join(root, "config.yml")]);
    expect(result.code, JSON.stringify(result)).toBe(0);
    expect(await readFile(path.join(target, "rooms", "old-room", "notes.md"), "utf8")).toBe("keep me");
    expect(await readFile(path.join(target, "operator.txt"), "utf8")).toBe("keep user file");
    // A module a newer release deleted must not survive inside the product tree.
    expect(await stat(path.join(target, ADAPTER_RELATIVE, "removed-in-this-release.ts")).catch(() => null)).toBeNull();
  });

  test("verification failure rolls back both the target and the config", async () => {
    const { root, zip } = await fixture({ verifier: "process.exit(1);\n" });
    const target = path.join(root, "existing");
    const config = path.join(root, "config.yml");
    await mkdir(target);
    await writeFile(path.join(target, "sentinel"), "original");
    await writeFile(config, "extensions:\n  - original.ts\n");

    const result = await run(["--bundle", zip, "--target", target, "--room", "demo", "--mode", "vault", "--force", "--config", config]);
    expect(result.code).not.toBe(0);
    expect(await readFile(path.join(target, "sentinel"), "utf8")).toBe("original");
    expect(await readFile(config, "utf8")).toBe("extensions:\n  - original.ts\n");
  });

  test("a verifier that reports the wrong mode is refused", async () => {
    const { root, zip } = await fixture({
      verifier: 'console.log(JSON.stringify({ ok: true, mode: "degraded" }));\n',
    });
    const target = path.join(root, "target");
    const result = await run(["--bundle", zip, "--target", target, "--room", "demo", "--mode", "vault", "--dry-run"]);
    expect(result.code).not.toBe(0);
    expect(result.stderr).toContain("not exactly Vault");
  });
});

describe.serial("installer 0.10.x migration", () => {
  test("a 0.10.x layout blocks a plain install and names the migration", async () => {
    const { root, zip } = await fixture();
    const target = await legacyTree(root);
    const result = await run(["--bundle", zip, "--target", target, "--room", "old-room", "--mode", "vault", "--force", "--config", path.join(root, "config.yml")]);
    expect(result.code).not.toBe(0);
    expect(result.stderr).toContain("--migrate-legacy");
    expect(await readFile(path.join(target, "rooms", "old-room", "notes.md"), "utf8")).toBe("irreplaceable operator notes");
  });

  test("migration preserves rooms, dotenv, backups, and manifests, and keeps rollback material", async () => {
    const { root, zip } = await fixture();
    const target = await legacyTree(root, { backup: "fresh" });
    const config = path.join(root, "config.yml");
    await writeFile(config, [
      "extensions:",
      `  - ${path.join(target, "solarisael-house-omp", "index.ts").replaceAll("\\", "/")}`,
      `  - ${path.join(target, "solarisael-house-omp", "hygiene.ts").replaceAll("\\", "/")}`,
      "  - unrelated.ts",
      "",
    ].join("\n"));

    const result = await run(["--bundle", zip, "--target", target, "--room", "old-room", "--mode", "vault", "--config", config, "--migrate-legacy"]);
    expect(result.code, JSON.stringify(result)).toBe(0);
    const output = JSON.parse(result.stdout);
    expect(output.migrated).toBe(true);

    expect(await readFile(path.join(target, "rooms", "old-room", "notes.md"), "utf8")).toBe("irreplaceable operator notes");
    expect(await readFile(path.join(target, "state", "substrate", ".env"), "utf8")).toContain("PGDATABASE=solarisael_memory");
    expect(await stat(path.join(target, "state", "substrate", "backups", "solarisael_memory.dump"))).toBeDefined();
    expect(await stat(path.join(target, "state", "legacy", "solarisael-house-omp", "package-manifest.json"))).toBeDefined();

    const directories = (await readdir(target, { withFileTypes: true })).filter((entry) => entry.isDirectory()).map((entry) => entry.name);
    for (const stale of ["solarisael-house", "solarisael-house-omp", "solarisael-house-substrate"]) {
      expect(directories).not.toContain(stale);
    }
    const rollback = directories.find((name) => name.startsWith(".athanor-rollback-0.10.x-"));
    expect(rollback).toBeString();
    expect(output.legacy.rollback).toContain(rollback as string);
    // Rollback material is retained, never deleted.
    expect(await stat(path.join(target, rollback as string, "solarisael-house-omp", "package-manifest.json"))).toBeDefined();
    expect(await readFile(path.join(target, rollback as string, "rooms", "old-room", "notes.md"), "utf8")).toBe("irreplaceable operator notes");

    const written = await readFile(config, "utf8");
    expect(written).toContain("unrelated.ts");
    expect(written).not.toContain("solarisael-house-omp");
    expect(written).toContain(path.join(await realpath(target), ADAPTER_RELATIVE, "index.ts").replaceAll("\\", "/"));
  });

  test("the legacy room identity beats pre-existing staged files at the preservation seam", async () => {
    const { root } = await fixture();
    const target = await legacyTree(root);
    const stage = path.join(root, "stage");
    const stagedRoom = path.join(stage, "rooms", "old-room");
    await mkdir(stagedRoom, { recursive: true });
    await writeFile(path.join(stagedRoom, ".solarisael-room.json"), JSON.stringify({
      version: 1,
      room: "old-room",
      trueName: "Example",
      operator: "Example",
    }));
    await writeFile(path.join(stagedRoom, "active_spirit.md"), "# Active Spirit: Example\nSTARTER IDENTITY\n");
    await writeFile(path.join(stagedRoom, "AGENTS.md"), "STARTER GUIDANCE\n");
    await writeFile(path.join(stagedRoom, "room_summary.md"), "STARTER SUMMARY\n");
    const legacyRooms = path.join(target, "rooms");
    const detection: LegacyDetection = {
      detected: true,
      requiresMigration: true,
      reasons: [],
      productDirectories: [],
      rooms: legacyRooms,
      substrateDirectory: null,
      preserve: [{ kind: "rooms", source: legacyRooms, destination: "rooms", overwrite: true }],
      staleExtensionPaths: [],
      topologyVariables: [],
    };

    await preserveLegacy(detection, stage);

    const spirit = await readFile(path.join(stagedRoom, "active_spirit.md"), "utf8");
    expect(spirit).toContain("LEGACY IDENTITY");
    expect(spirit).not.toContain("STARTER IDENTITY");
    expect(await readFile(path.join(stagedRoom, "AGENTS.md"), "utf8")).toContain("LEGACY GUIDANCE");
    expect(await readFile(path.join(stagedRoom, "room_summary.md"), "utf8")).toBe("LEGACY SUMMARY\n");
    expect(await readFile(path.join(stagedRoom, "notes.md"), "utf8")).toBe("irreplaceable operator notes");
    expect(JSON.parse(await readFile(path.join(stagedRoom, ".solarisael-room.json"), "utf8")))
      .toMatchObject({ room: "old-room", trueName: "Old Room", operator: "Sol" });
  });

  for (const [label, backup, expected] of [
    ["no backup at all", "none", "no PostgreSQL backup was found"],
    ["a file that is not a pg_dump archive", "corrupt", "missing PGDMP header"],
    ["a backup older than the freshness window", "stale", "freshness window"],
  ] as const) {
    test(`AKASHA migration refuses ${label}`, async () => {
      const { root, zip } = await fixture({ profile: "akasha" });
      const target = await legacyTree(root, { backup });
      const result = await run(["--bundle", zip, "--target", target, "--room", "old-room", "--mode", "akasha", "--config", path.join(root, "config.yml"), "--migrate-legacy"]);
      expect(result.code).not.toBe(0);
      expect(result.stderr).toContain(expected);
      // Refusal happens before any activation.
      expect(await stat(path.join(target, "solarisael-house-omp")).catch(() => null)).not.toBeNull();
      expect(await stat(path.join(target, PRODUCT_DIRECTORY)).catch(() => null)).toBeNull();
    });
  }

  for (const [label, withScript] of [["a detected 0.10.x backup.sh", true], ["no backup script anywhere", false]] as const) {
    test(`the backup refusal names a command that exists now, given ${label}`, async () => {
      const { root, zip } = await fixture({ profile: "akasha" });
      const target = await legacyTree(root);
      const legacyScript = path.join(target, "solarisael-house-substrate", "backup.sh");
      if (withScript) await writeFile(legacyScript, "#!/usr/bin/env bash\n");

      const result = await run(["--bundle", zip, "--target", target, "--room", "old-room", "--mode", "akasha", "--config", path.join(root, "config.yml"), "--migrate-legacy"]);
      expect(result.code).not.toBe(0);
      // Before migration <target>/the-athanor does not exist. Pointing the
      // operator through it would name a script they cannot run.
      expect(result.stderr).not.toContain(`${PRODUCT_DIRECTORY}/substrate/backup.sh`);
      expect(result.stderr).not.toContain("the-athanor\\\\substrate");
      if (withScript) {
        expect(result.stderr).toContain("backup.sh");
        expect(await stat(legacyScript)).toBeDefined();
      } else {
        expect(result.stderr).toContain("pg_dump");
        expect(result.stderr).toContain("solarisael_memory");
        expect(result.stderr).toContain("-Fc");
      }
    });
  }

  test("AKASHA migration proceeds once a fresh custom-format dump exists", async () => {
    const { root, zip } = await fixture({ profile: "akasha" });
    const target = await legacyTree(root, { backup: "fresh" });
    const result = await run(["--bundle", zip, "--target", target, "--room", "old-room", "--mode", "akasha", "--config", path.join(root, "config.yml"), "--migrate-legacy"]);
    expect(result.code, JSON.stringify(result)).toBe(0);
    const output = JSON.parse(result.stdout);
    expect(output.profile).toBe("akasha");
    expect(output.backup.source).toContain("solarisael_memory.dump");
    // A migration preserves its dump in place rather than copying one in.
    expect(output.backup.destination).toBeNull();
    expect(output.environment.ATHANOR_SUBSTRATE_ROOT).toBe(path.join(await realpath(target), SUBSTRATE_RELATIVE));
    expect(output.environment.ATHANOR_SUBSTRATE_EXE).toBe(path.join(await realpath(target), ADAPTER_RELATIVE, substrateBinaryRelative(platform!)));
    expect(await stat(path.join(target, "state", "substrate"))).toBeDefined();

    const environmentFile = await readFile(path.join(target, "state", "athanor.env"), "utf8");
    const keys = environmentFile.split(/\r?\n/).filter((line) => line && !line.startsWith("#")).map((line) => line.split("=", 1)[0]);
    expect(keys.sort()).toEqual(["ATHANOR_STATE_DIR", "ATHANOR_SUBSTRATE_EXE", "ATHANOR_SUBSTRATE_ROOT"]);
    expect(environmentFile).toContain(`ATHANOR_SUBSTRATE_ROOT=${path.join(await realpath(target), SUBSTRATE_RELATIVE)}`);
  });

  test("an explicit --backup outside the install root is accepted", async () => {
    const { root, zip } = await fixture({ profile: "akasha" });
    const target = await legacyTree(root);
    const dump = path.join(root, "operator-taken.dump");
    await writeFile(dump, Buffer.concat([Buffer.from("PGDMP"), Buffer.alloc(32, 9)]));
    const result = await run(["--bundle", zip, "--target", target, "--room", "old-room", "--mode", "akasha", "--config", path.join(root, "config.yml"), "--migrate-legacy", "--backup", dump]);
    expect(result.code, JSON.stringify(result)).toBe(0);
    expect(JSON.parse(result.stdout).backup.source).toBe(dump);
    expect(JSON.parse(result.stdout).backup.destination).toBeNull();
  });

  test("--mode full is accepted only alongside --migrate-legacy and maps to akasha", async () => {
    const { root, zip } = await fixture({ profile: "akasha" });
    const target = await legacyTree(root, { backup: "fresh" });
    const result = await run(["--bundle", zip, "--target", target, "--room", "old-room", "--mode", "full", "--config", path.join(root, "config.yml"), "--migrate-legacy"]);
    expect(result.code, JSON.stringify(result)).toBe(0);
    expect(JSON.parse(result.stdout).profile).toBe("akasha");
  });
});

describe.serial("installer packaging", () => {
  test("installer source compiles for the current platform", async () => {
    const result = await new Promise<{ code: number }>((resolve, reject) => {
      const child = spawn("bun", ["build", "--compile", installer, "--outfile", path.join(os.tmpdir(), `athanor-installer-${Date.now()}.exe`)], { windowsHide: true });
      child.on("error", reject);
      child.on("exit", (code) => resolve({ code: code ?? -1 }));
    });
    expect(result.code).toBe(0);
  });
});

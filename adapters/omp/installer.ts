// The one public installation door.
//
// An install lays down exactly three things at <target>:
//
//   the-athanor/    the immutable product tree, adapter at adapters/omp
//   rooms/          the operator's rooms
//   state/          mutable state; state/substrate holds the dotenv and dumps
//
// Two public profiles: `vault` (no substrate, no PostgreSQL, no Rust runtime)
// and `akasha` (substrate operations plus the platform substrate binary). The
// 0.10.x `base`/`full` tokens are gone from this CLI; only the legacy migration
// parser still knows them.

import { cp, lstat, mkdir, mkdtemp, readdir, readFile, realpath, rename, rm, writeFile } from "node:fs/promises";
import { HARNESS_DESCRIPTORS, selectHarnesses, type HarnessId } from "./harnesses.ts";
import os from "node:os";
import path from "node:path";
import { spawn } from "node:child_process";
import {
  DEVELOPMENT_MARKER_ENTRIES,
  DEVELOPMENT_MARKER_PREFIXES,
  EXTENSION_PATH_PATTERN,
  LEGACY_PRODUCT_DIRECTORIES,
  LEGACY_TOPOLOGY_VARIABLES,
  PRODUCT_DIRECTORY,
  REQUIRED_ENTRIES,
  VAULT_FORBIDDEN_ENTRIES,
  VAULT_FORBIDDEN_PREFIXES,
  akashaRequiredEntries,
  archivePlatform,
  extensionPaths,
  isProfile,
  layout,
  substrateBinaryRelative,
  type ArchivePlatform,
  type Profile,
} from "./install-layout.ts";
import {
  backupInstruction,
  backupSearchDirectories,
  detectLegacyInstall,
  legacyModeToken,
  readConfigText,
  requireDatabaseBackup,
  type BackupProof,
  type LegacyDetection,
} from "./install-legacy.ts";

type Options = {
  bundle: string;
  target: string;
  room: string;
  profile: Profile;
  force: boolean;
  dryRun: boolean;
  update: boolean;
  migrateLegacy: boolean;
  backup?: string;
  backupMaxAgeHours: number;
  /** Source dotenv seeded into state/substrate/.env. Fresh AKASHA only. */
  envFile?: string;
  config: string;
  harnesses: HarnessId[];
};

type Result = {
  ok: boolean;
  target: string;
  profile?: Profile;
  room?: string;
  harnesses?: HarnessId[];
  dryRun?: boolean;
  updated?: boolean;
  migrated?: boolean;
  environment?: Record<string, string>;
  /**
   * The proven recovery point. `destination` is null for a legacy migration,
   * where the dump is preserved in place rather than copied in.
   */
  backup?: { source: string; destination: string | null; size: number; modifiedAt: string };
  legacy?: { reasons: string[]; preserved: string[]; rollback: string | null };
  warning?: string;
  error?: string;
};

const usage = (): never => {
  throw new Error("Usage: installer.ts --bundle ZIP --target DIR --room ROOM --mode vault|akasha [--harness omp] [--config ABSOLUTE_PATH] [--env-file ABSOLUTE_PATH] [--migrate-legacy] [--backup DUMP] [--backup-max-age-hours N] [--force] [--update] [--dry-run], or --list-harnesses");
};
const isAbsolute = (v: string) => path.isAbsolute(v) || /^[A-Za-z]:[\\/]/.test(v) || /^\\\\/.test(v);

function parseArgs(argv: string[]): Options {
  const values = new Map<string, string>();
  const harnessValues: string[] = [];
  let force = false;
  let dryRun = false;
  let update = false;
  let migrateLegacy = false;
  for (let i = 0; i < argv.length; i++) {
    const argument = argv[i] as string;
    if (argument === "--force") { force = true; continue; }
    if (argument === "--dry-run") { dryRun = true; continue; }
    if (argument === "--update") { update = true; continue; }
    if (argument === "--migrate-legacy") { migrateLegacy = true; continue; }
    if (!["--bundle", "--target", "--room", "--mode", "--config", "--harness", "--backup", "--backup-max-age-hours", "--env-file"].includes(argument)) usage();
    const value = argv[++i];
    if (!value || value.startsWith("--")) usage();
    if (argument === "--harness") harnessValues.push(value);
    else values.set(argument, value);
  }
  const bundle = values.get("--bundle");
  const target = values.get("--target");
  const room = values.get("--room");
  const requestedMode = values.get("--mode") || "";
  const config = values.get("--config") || path.join(os.homedir(), ".omp", "agent", "config.yml");
  if (!bundle || !target || !room) usage();

  // `base`/`full` are not public modes. They are recognised only so that a
  // migration invocation can name the profile it used to mean, and only when
  // the operator has explicitly asked for the legacy migration.
  const legacyProfile = legacyModeToken(requestedMode);
  if (legacyProfile && !migrateLegacy) {
    throw new Error(`--mode ${requestedMode} is a 0.10.x token; the public modes are vault and akasha. Rerun with --migrate-legacy --mode ${legacyProfile} to migrate a 0.10.x install.`);
  }
  const profile = isProfile(requestedMode) ? requestedMode : legacyProfile;
  if (!profile) usage();

  if (!isAbsolute(target) || !isAbsolute(config)) throw new Error("--target and --config must be absolute paths");
  if (!/^[a-z0-9]+(?:-[a-z0-9]+)*$/.test(room) || room === "house") throw new Error("--room must be a safe non-reserved slug");
  const maxAge = Number(values.get("--backup-max-age-hours") || "24");
  if (!Number.isFinite(maxAge) || maxAge <= 0) throw new Error("--backup-max-age-hours must be a positive number");

  // --env-file seeds state/substrate/.env for a FRESH AKASHA install, which
  // otherwise has no honest way to hold substrate credentials before the staged
  // verification runs. Vault has no substrate and must never accept one; a
  // legacy migration already carries the detected 0.10.x dotenv forward.
  const envFile = values.get("--env-file");
  if (envFile) {
    if (profile !== "akasha") throw new Error("--env-file is only accepted for --mode akasha; Vault installs have no substrate dotenv");
    if (!isAbsolute(envFile)) throw new Error("--env-file must be an absolute path");
    if (migrateLegacy) throw new Error("--env-file is not accepted with --migrate-legacy; the detected 0.10.x substrate dotenv is preserved instead");
  }

  // Vault has no database, so a backup input is always a category error there —
  // accepting it silently would imply a safety step that cannot exist.
  const backup = values.get("--backup");
  if (backup && profile !== "akasha") {
    throw new Error("--backup is only accepted for --mode akasha; Vault installs have no PostgreSQL database to back up");
  }
  if (backup && !isAbsolute(backup)) throw new Error("--backup must be an absolute path");
  if (values.has("--backup-max-age-hours") && profile !== "akasha") {
    throw new Error("--backup-max-age-hours is only accepted for --mode akasha");
  }

  return {
    bundle: path.resolve(bundle),
    target: path.resolve(target),
    room,
    profile,
    force,
    dryRun,
    update,
    migrateLegacy,
    backup: backup ? path.resolve(backup) : undefined,
    backupMaxAgeHours: maxAge,
    envFile: envFile ? path.resolve(envFile) : undefined,
    config: path.resolve(config),
    harnesses: selectHarnesses(harnessValues),
  };
}

/**
 * Resolves on `close`, not `exit`.
 *
 * `exit` fires when the process ends, which can precede the final stdout
 * chunks; `close` fires only once every stdio stream is drained. Under load a
 * truncated `tar -tf` listing would silently shorten the entry list — making a
 * good bundle look like it is missing required files, and worse, making a
 * forbidden entry look absent. Every safety decision here reads this output.
 */
function exec(command: string, args: string[], cwd?: string, env?: NodeJS.ProcessEnv): Promise<{ code: number; stdout: string; stderr: string }> {
  return new Promise((resolve, reject) => {
    const c = spawn(command, args, { cwd, env, windowsHide: true });
    let stdout = "", stderr = "";
    let code = -1;
    c.stdout?.on("data", (d) => stdout += d);
    c.stderr?.on("data", (d) => stderr += d);
    c.on("error", reject);
    c.on("exit", (value) => { code = value ?? -1; });
    c.on("close", () => resolve({ code, stdout, stderr }));
  });
}

function safeEntry(entry: string) {
  const n = entry.replaceAll("\\", "/").replace(/^\.\/+/, "").replace(/\/+$/, "");
  return n === "" || (n !== "." && !n.startsWith("/") && !/^[A-Za-z]:/.test(n) && !n.split("/").includes(".."));
}

async function archiveEntries(bundle: string) {
  const r = await exec("tar", ["-tf", bundle]);
  if (r.code) throw new Error(`unable to read bundle: ${r.stderr || r.stdout}`);
  const e = r.stdout.split(/\r?\n/).map((x) => x.trim()).filter(Boolean);
  for (const x of e) if (!safeEntry(x)) throw new Error(`unsafe archive entry: ${x}`);
  return e.map((x) => x.replaceAll("\\", "/").replace(/^\.\/+/, "").replace(/\/+$/, "")).filter(Boolean);
}

/** The archive must match the requested profile exactly, in both directions. */
function assertProfileLayout(entries: string[], profile: Profile, platform: ArchivePlatform | null): void {
  const present = new Set(entries);
  for (const entry of REQUIRED_ENTRIES) {
    if (!present.has(entry)) throw new Error(`bundle missing required file: ${entry}`);
  }
  for (const name of LEGACY_PRODUCT_DIRECTORIES) {
    if (entries.some((entry) => entry === name || entry.startsWith(`${name}/`))) {
      throw new Error(`bundle carries a 0.10.x product directory: ${name}`);
    }
  }
  // Applies to BOTH profiles and must run before the akasha early return: a
  // shipped Cargo.toml plus crates/ would make the installed tree resolve its
  // state root through the substrate's development-checkout fallback.
  for (const entry of DEVELOPMENT_MARKER_ENTRIES) {
    if (present.has(entry)) throw new Error(`bundle carries a development-checkout marker: ${entry}`);
  }
  for (const prefix of DEVELOPMENT_MARKER_PREFIXES) {
    const offender = entries.find((entry) => entry.startsWith(prefix));
    if (offender) throw new Error(`bundle carries a development-checkout marker: ${offender}`);
  }
  if (profile === "akasha") {
    if (!platform) throw new Error(`unsupported AKASHA platform: ${process.platform}-${process.arch}`);
    for (const entry of akashaRequiredEntries(platform)) {
      if (!present.has(entry)) throw new Error(`AKASHA bundle missing required file: ${entry}`);
    }
    return;
  }
  for (const entry of VAULT_FORBIDDEN_ENTRIES) {
    if (present.has(entry)) throw new Error(`Vault bundle must not contain: ${entry}`);
  }
  for (const prefix of VAULT_FORBIDDEN_PREFIXES) {
    const offender = entries.find((entry) => entry.startsWith(prefix));
    if (offender) throw new Error(`Vault bundle must not contain substrate assets: ${offender}`);
  }
}

async function noSymlinks(root: string) {
  const walk = async (d: string): Promise<void> => {
    for (const x of await readdir(d, { withFileTypes: true })) {
      const f = path.join(d, x.name);
      const i = await lstat(f);
      if (i.isSymbolicLink()) throw new Error(`symlink archive entry refused: ${path.relative(root, f)}`);
      if (i.isDirectory()) await walk(f);
    }
  };
  await walk(root);
}

/**
 * Carry forward files the operator owns without dragging the previous product
 * tree along. The product directory is replaced wholesale — merging it would
 * resurrect modules a newer release deleted.
 */
async function mergeMissing(from: string, into: string, skip?: (name: string) => boolean) {
  for (const x of await readdir(from, { withFileTypes: true })) {
    if (skip?.(x.name)) continue;
    const s = path.join(from, x.name), d = path.join(into, x.name);
    const existing = await lstat(d).catch(() => null);
    if (existing) {
      if (x.isDirectory() && existing.isDirectory()) await mergeMissing(s, d);
    } else {
      await cp(s, d, { recursive: x.isDirectory() });
    }
  }
}

const preservedProductNoise = (name: string) =>
  name === PRODUCT_DIRECTORY || name.startsWith(".athanor-") || name === "athanor.env";

async function configure(root: string, room: string) {
  const paths = layout(root);
  const roomDir = path.join(paths.rooms, room);
  await mkdir(paths.rooms, { recursive: true });
  await cp(path.join(paths.adapter, "starter-room", "example"), roomDir, { recursive: true, force: true });
  const markerPath = path.join(roomDir, ".solarisael-room.json");
  const marker = JSON.parse(await readFile(markerPath, "utf8"));
  marker.room = room;
  marker.trueName = room;
  marker.operator = process.env.USERNAME || process.env.USER || "Operator";
  await writeFile(markerPath, JSON.stringify(marker, null, 2) + "\n");
  const spiritPath = path.join(roomDir, "active_spirit.md");
  const spirit = (await readFile(spiritPath, "utf8"))
    .replaceAll("\r\n", "\n")
    .replace(/^# Active Spirit:.*$/m, `# Active Spirit: ${marker.trueName}`)
    .replace(/^Agent:.*$/m, `Agent: ${marker.trueName} | Operator: ${marker.operator}`)
    .replace(/^# SPIRIT:.*$/m, `# SPIRIT: ${marker.trueName}`);
  await writeFile(spiritPath, spirit);
  return roomDir;
}

/**
 * `solarisael_memory_<UTC timestamp>.dump` — the same rotation family
 * `substrate/backup.sh` writes and prunes (it globs `${PGDATABASE}_*.dump`).
 * A seeded safety net keeping an operator's odd basename would be immortal:
 * rotation would never see it, and it would outlive its usefulness forever.
 */
function stagedBackupName(now = new Date()): string {
  const pad = (value: number) => String(value).padStart(2, "0");
  const stamp = `${now.getUTCFullYear()}-${pad(now.getUTCMonth() + 1)}-${pad(now.getUTCDate())}`
    + `_${pad(now.getUTCHours())}${pad(now.getUTCMinutes())}${pad(now.getUTCSeconds())}`;
  return `solarisael_memory_${stamp}.dump`;
}

/**
 * Strip every generation of Athanor extension path — 0.10.x and current — then
 * add exactly the canonical pair. Unrelated extensions and settings survive.
 */
export function wireConfig(text: string, root: string): string {
  const canonicalPaths = extensionPaths(root);
  const lines = (text ? text.split(/\r?\n/) : []).filter((line) => {
    const value = line.trim().replace(/^-\s*/, "").replace(/^(['"])(.*)\1$/, "$2");
    return !EXTENSION_PATH_PATTERN.test(value);
  });
  if (!lines.some((line) => /^extensions:\s*$/.test(line.trim()))) lines.push("extensions:");
  let index = lines.findIndex((line) => /^extensions:\s*$/.test(line.trim())) + 1;
  for (const value of canonicalPaths) lines.splice(index++, 0, `  - ${value}`);
  return lines.join("\n").replace(/\n*$/, "\n");
}

/** The canonical topology variables, and nothing else. */
function topologyEnvironment(root: string, profile: Profile, platform: ArchivePlatform | null): Record<string, string> {
  const paths = layout(root);
  const environment: Record<string, string> = { ATHANOR_STATE_DIR: paths.state };
  if (profile === "akasha" && platform) {
    environment.ATHANOR_SUBSTRATE_ROOT = paths.substrate;
    environment.ATHANOR_SUBSTRATE_EXE = path.join(paths.adapter, substrateBinaryRelative(platform));
  }
  return environment;
}

function childEnvironment(topology: Record<string, string>): NodeJS.ProcessEnv {
  const environment: NodeJS.ProcessEnv = { ...process.env };
  for (const name of LEGACY_TOPOLOGY_VARIABLES) delete environment[name];
  delete environment.ATHANOR_SUBSTRATE_ROOT;
  delete environment.ATHANOR_SUBSTRATE_EXE;
  delete environment.ATHANOR_STATE_DIR;
  delete environment.ATHANOR_AUTO;
  return { ...environment, ...topology };
}

/**
 * The installed adapter reads this file structurally before any topology
 * lookup, so a fresh shell needs nothing exported. It lives in mutable state,
 * never inside the immutable product tree.
 */
async function writeEnvironmentFile(root: string, topology: Record<string, string>): Promise<void> {
  const target = layout(root).environmentFile;
  const body = [
    "# Written by the Athanor installer. Canonical topology for this install.",
    "# The adapter loads this file structurally; the real environment wins over it.",
    ...Object.entries(topology).map(([name, value]) => `${name}=${value}`),
    "",
  ].join("\n");
  await mkdir(path.dirname(target), { recursive: true });
  await writeFile(target, body, "utf8");
}

async function verify(root: string, options: Options, topology: Record<string, string>, configPath: string) {
  const paths = layout(root);
  const bun = /(?:^|[\\/])bun(?:\.exe)?$/i.test(process.execPath) ? process.execPath : "bun";
  const args = [
    path.join(paths.adapter, "verify-install.ts"),
    "--room", path.join(paths.rooms, options.room),
    "--config", configPath,
    "--profile", options.profile,
    "--require-manifest",
  ];
  return await exec(bun, args, root, childEnvironment(topology));
}

function assertVerification(label: string, result: { code: number; stdout: string; stderr: string }, profile: Profile): void {
  if (result.code) throw new Error(`${label} verification failed: ${result.stdout || result.stderr}`);
  const parsed = JSON.parse(result.stdout.trim());
  const expected = profile === "akasha" ? "AKASHA" : "Vault";
  if (parsed.mode !== expected) {
    throw new Error(`${label} verification refused: verifier mode is ${String(parsed.mode)}, not exactly ${expected}`);
  }
}

/**
 * Exported so the collision policy can be tested at the seam. Through the CLI
 * the staged starter happens to be written after this runs, which hides whether
 * the policy or merely the ordering is protecting room identity.
 */
export async function preserveLegacy(detection: LegacyDetection, stage: string): Promise<string[]> {
  const preserved: string[] = [];
  for (const item of detection.preserve) {
    const destination = path.join(stage, item.destination);
    await mkdir(path.dirname(destination), { recursive: true });
    const isDirectory = Boolean((await lstat(item.source).catch(() => null))?.isDirectory());
    if (isDirectory) {
      await mkdir(destination, { recursive: true });
      // Rooms overwrite: the staged starter ships active_spirit.md, AGENTS.md,
      // room_summary.md and a marker under the SAME names, with a different
      // identity. Merge-missing would keep the starter's copies and splice a
      // stranger's identity into a real room, leaving marker and spirit
      // disagreeing. Everything else keeps the staged copy on collision.
      if (item.overwrite) await cp(item.source, destination, { recursive: true, force: true });
      else await mergeMissing(item.source, destination);
    } else if (item.overwrite || !(await lstat(destination).catch(() => null))) {
      await cp(item.source, destination, { force: true });
    }
    preserved.push(`${item.kind}: ${item.source} -> ${item.destination.replaceAll("\\", "/")}`);
  }
  return preserved;
}

async function main(): Promise<Result> {
  const options = parseArgs(process.argv.slice(2));
  const platform = archivePlatform();
  const exists = Boolean(await lstat(options.target).catch(() => null));
  const existingConfig = await readConfigText(options.config);
  const detection = await detectLegacyInstall({ target: options.target, configText: existingConfig, env: process.env });

  if (detection.requiresMigration && !options.migrateLegacy) {
    throw new Error([
      "A 0.10.x installation was detected and this installer will not silently replace it.",
      ...detection.reasons,
      `Rerun with --migrate-legacy --mode ${options.profile} to migrate it; rooms, configuration, dotenv, and backups are preserved and the old directories are kept under a rollback path.`,
    ].join(" "));
  }
  if (options.migrateLegacy && !detection.detected) {
    throw new Error("--migrate-legacy was requested but no 0.10.x layout, configuration, or topology variable was found.");
  }
  if (options.update && !exists) throw new Error("update target does not exist");
  if (exists && !options.force && !options.update && !options.migrateLegacy) {
    throw new Error("target already exists; pass --force to replace it, --update to preserve it, or --migrate-legacy to migrate a 0.10.x install");
  }

  const bundleInfo = await lstat(options.bundle).catch(() => null);
  if (!bundleInfo?.isFile()) throw new Error("bundle must be a regular file");
  assertProfileLayout(await archiveEntries(options.bundle), options.profile, platform);

  if (!options.dryRun) await mkdir(path.dirname(options.target), { recursive: true });
  const temp = await mkdtemp(path.join(options.dryRun ? os.tmpdir() : path.dirname(options.target), ".athanor-install-"));
  const stage = path.join(temp, "install");
  let hold: string | undefined;
  let rollback: string | null = null;
  let configBackup: string | undefined;
  let targetCommitted = false;
  let configCommitted = false;
  let warning: string | undefined;
  let preserved: string[] = [];
  let backupProof: BackupProof | null = null;
  let backupDestination: string | null = null;

  try {
    await mkdir(stage, { recursive: true });
    const extraction = await exec("tar", ["-xf", options.bundle, "-C", stage]);
    if (extraction.code) throw new Error(`bundle extraction failed: ${extraction.stderr || extraction.stdout}`);
    await noSymlinks(stage);

    const stagePaths = layout(stage);
    const manifest = JSON.parse(await readFile(path.join(stagePaths.adapter, "package-manifest.json"), "utf8"));
    if (manifest.profile !== options.profile) {
      throw new Error(`bundle profile is ${String(manifest.profile)}, not ${options.profile}`);
    }
    if (platform && manifest.platform !== platform) {
      throw new Error(`bundle platform is ${String(manifest.platform)}, not ${platform}`);
    }

    await mkdir(stagePaths.rooms, { recursive: true });
    await mkdir(stagePaths.state, { recursive: true });
    if (options.profile === "akasha") await mkdir(stagePaths.substrateState, { recursive: true });

    if (options.migrateLegacy) {
      preserved = await preserveLegacy(detection, stage);
      if (options.profile === "akasha") {
        backupProof = await requireDatabaseBackup({
          explicit: options.backup,
          searchDirectories: backupSearchDirectories(options.target, detection),
          maxAgeHours: options.backupMaxAgeHours,
          // Must name something that exists BEFORE the migration; the new
          // product tree does not exist yet when this refusal is printed.
          backupCommand: await backupInstruction(detection, layout(options.target).substrateBackups),
        });
      }
    } else if (exists) {
      await mergeMissing(options.target, stage, preservedProductNoise);
    }

    // Every AKASHA activation needs a real recovery point, fresh installs
    // included. "Fresh" is not a loophole: the operator prepares the database,
    // applies migrations, takes the dump, then installs. An absent or empty
    // backups directory is the absence of safety, not the presence of it, so a
    // fresh install requires --backup explicitly rather than scanning for one.
    if (options.profile === "akasha" && !options.migrateLegacy && !options.update) {
      backupProof = await requireDatabaseBackup({
        explicit: options.backup,
        searchDirectories: [],
        maxAgeHours: options.backupMaxAgeHours,
        backupCommand: await backupInstruction(detection, layout(options.target).substrateBackups),
      });
      await mkdir(stagePaths.substrateBackups, { recursive: true });
      // Normalised into backup.sh's rotation family so the seeded safety net is
      // pruned like any other dump. The operator's source file is untouched.
      backupDestination = path.join(layout(options.target).substrateBackups, stagedBackupName());
      await cp(backupProof.path, path.join(stagePaths.substrateBackups, path.basename(backupDestination)), { force: true });
    }

    // Seed substrate credentials into staged mutable state so the staged AKASHA
    // verification has a real dotenv to read, and so it survives activation via
    // the stage -> target rename. A migration already carried one forward.
    if (options.envFile) {
      const source = await lstat(options.envFile).catch(() => null);
      if (!source?.isFile()) throw new Error(`--env-file must be a readable regular file: ${options.envFile}`);
      await readFile(options.envFile, "utf8");
      await mkdir(stagePaths.substrateState, { recursive: true });
      await cp(options.envFile, stagePaths.substrateDotenv, { force: true });
    } else if (options.profile === "akasha" && !options.migrateLegacy && !options.update) {
      const seeded = await lstat(stagePaths.substrateDotenv).catch(() => null);
      if (!seeded?.isFile()) {
        throw new Error(`fresh AKASHA needs substrate credentials: pass --env-file ABSOLUTE_PATH pointing at a dotenv to install as ${stagePaths.substrateDotenv.replace(stage, options.target)}`);
      }
    }

    const roomMarker = path.join(stagePaths.rooms, options.room, ".solarisael-room.json");
    const roomPresent = Boolean((await lstat(roomMarker).catch(() => null))?.isFile());
    if (options.update && !roomPresent) throw new Error(`update room is missing its marker: ${roomMarker}`);
    if (!roomPresent) await configure(stage, options.room);

    const ompSelected = options.harnesses.includes("omp");
    const canonicalStage = await realpath(stage);
    const canonicalTarget = path.join(await realpath(path.dirname(options.target)), path.basename(options.target));
    const proposed = ompSelected ? wireConfig(existingConfig, canonicalTarget) : existingConfig;
    const proposedConfig = path.join(temp, "config.yml");
    await writeFile(proposedConfig, wireConfig(existingConfig, canonicalStage));

    const stageTopology = topologyEnvironment(canonicalStage, options.profile, platform);
    await writeEnvironmentFile(stage, stageTopology);
    assertVerification("staged bundle", await verify(stage, options, stageTopology, proposedConfig), options.profile);

    // Topology must be canonical: the verifier resolves its own roots through
    // the real filesystem, so a short-name (8.3) or symlinked --target would
    // otherwise compare unequal against ATHANOR_STATE_DIR.
    const targetTopology = topologyEnvironment(canonicalTarget, options.profile, platform);
    if (options.dryRun) {
      return {
        ok: true,
        target: options.target,
        profile: options.profile,
        room: options.room,
        harnesses: options.harnesses,
        dryRun: true,
        updated: options.update,
        migrated: options.migrateLegacy,
        environment: targetTopology,
        ...(backupProof
          ? { backup: { source: backupProof.path, destination: backupDestination, size: backupProof.size, modifiedAt: backupProof.modifiedAt } }
          : {}),
        ...(detection.detected
          ? { legacy: { reasons: detection.reasons, preserved, rollback: null } }
          : {}),
      };
    }

    if (exists) {
      hold = path.join(path.dirname(options.target), `.athanor-previous-${Date.now()}`);
      await rename(options.target, hold);
    }
    await rename(stage, options.target);
    targetCommitted = true;
    await writeEnvironmentFile(options.target, targetTopology);

    if (ompSelected) {
      await mkdir(path.dirname(options.config), { recursive: true });
      if (await lstat(options.config).catch(() => null)) {
        configBackup = `${options.config}.backup-${Date.now()}`;
        await rename(options.config, configBackup);
      }
      await writeFile(options.config, proposed);
      configCommitted = true;
    }

    const finalConfig = ompSelected ? options.config : proposedConfig;
    assertVerification("installed bundle", await verify(options.target, options, targetTopology, finalConfig), options.profile);

    if (hold) {
      if (options.migrateLegacy) {
        // Proof first, retirement second, deletion never. The 0.10.x tree keeps
        // its shape under a visible name inside the new install.
        rollback = path.join(options.target, `.athanor-rollback-0.10.x-${Date.now()}`);
        try { await rename(hold, rollback); }
        catch (error) {
          rollback = hold;
          warning = `legacy tree left at ${hold}: ${error instanceof Error ? error.message : String(error)}`;
        }
      } else {
        try { await rm(hold, { recursive: true, force: true }); }
        catch (error) { warning = `previous install cleanup failed: ${error instanceof Error ? error.message : String(error)}`; }
      }
      hold = undefined;
    }
    if (configBackup) {
      try { await rm(configBackup, { recursive: true, force: true }); }
      catch (error) { warning = `backup cleanup failed: ${error instanceof Error ? error.message : String(error)}`; }
    }

    return {
      ok: true,
      target: options.target,
      profile: options.profile,
      room: options.room,
      harnesses: options.harnesses,
      updated: options.update,
      migrated: options.migrateLegacy,
      environment: targetTopology,
      ...(backupProof
        ? { backup: { source: backupProof.path, destination: backupDestination, size: backupProof.size, modifiedAt: backupProof.modifiedAt } }
        : {}),
      ...(detection.detected
        ? { legacy: { reasons: detection.reasons, preserved, rollback } }
        : {}),
      ...(warning ? { warning } : {}),
    };
  } catch (error) {
    if (configCommitted) await rm(options.config, { force: true }).catch(() => {});
    if (configBackup) await rename(configBackup, options.config).catch(() => {});
    if (targetCommitted) await rm(options.target, { recursive: true, force: true }).catch(() => {});
    if (hold) await rename(hold, options.target).catch(() => {});
    throw error;
  } finally {
    await rm(temp, { recursive: true, force: true }).catch(() => {});
  }
}

if (import.meta.main) {
  try {
    if (process.argv.includes("--list-harnesses")) {
      console.log(JSON.stringify({ ok: true, harnesses: HARNESS_DESCRIPTORS }));
    } else {
      console.log(JSON.stringify(await main()));
    }
  } catch (error) {
    console.error(JSON.stringify({
      ok: false,
      target: process.argv[process.argv.indexOf("--target") + 1] || "",
      error: error instanceof Error ? error.message : String(error),
    }));
    process.exitCode = 1;
  }
}

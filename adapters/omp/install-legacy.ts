// The 0.10.x door.
//
// Before the unified product boundary, an install root held three sibling
// product directories — solarisael-house, solarisael-house-omp, and
// solarisael-house-substrate — the OMP config pointed at
// solarisael-house-omp/{index,hygiene}.ts, and topology came from
// SOLARISAEL_* variables. This module is the ONLY place those names are still
// allowed to appear outside their own tests: it recognises the old shape,
// gathers everything that must survive, and proves a database backup exists
// before an AKASHA install is allowed to activate.
//
// It never deletes anything. Retiring old directories is the installer's job,
// and it moves them to a visible rollback location.

import { lstat, open, readdir, readFile, stat } from "node:fs/promises";
import path from "node:path";
import {
  EXTENSION_PATH_PATTERN,
  LEGACY_PRODUCT_DIRECTORIES,
  LEGACY_TOPOLOGY_VARIABLES,
  layout,
  type Profile,
} from "./install-layout.ts";

export type PreservedKind = "rooms" | "dotenv" | "backups" | "package-manifest" | "configuration";

export type PreservedItem = {
  kind: PreservedKind;
  /** Absolute source path in the legacy tree. */
  source: string;
  /** Path relative to the new install root where this lands. */
  destination: string;
  /**
   * The operator's copy wins every collision with staged content.
   *
   * Only room data sets this. A room's identity lives across several files —
   * marker, active_spirit.md, AGENTS.md, room_summary.md — and the staged
   * starter room contains files with the SAME names and a different identity.
   * Keeping the staged copy on collision would splice a starter identity into
   * a real room and leave marker and spirit disagreeing. Product code is still
   * replaced wholesale; this is only about user data.
   */
  overwrite?: boolean;
};

export type LegacyDetection = {
  /** Any 0.10.x signal at all: directories, stale config, or old variables. */
  detected: boolean;
  /**
   * A 0.10.x product directory sits at the target. Only this blocks an install,
   * because only this holds data that a plain replace would destroy. Stale
   * config paths and stray SOLARISAEL_* variables in an operator's shell are
   * advisory: the installer strips and scrubs them without ceremony.
   */
  requiresMigration: boolean;
  /** Human-readable evidence, one line per signal. Surfaced in the install result. */
  reasons: string[];
  /** Absolute 0.10.x product directories found at the install root. */
  productDirectories: string[];
  /** Absolute path of the legacy rooms directory, if any. */
  rooms: string | null;
  /** Absolute legacy substrate directory, if any. */
  substrateDirectory: string | null;
  /** Material that must survive the migration. */
  preserve: PreservedItem[];
  /** Stale extension paths found in the operator's OMP config. */
  staleExtensionPaths: string[];
  /** 0.10.x topology variables present in the environment. */
  topologyVariables: string[];
};

const directoryExists = async (candidate: string) =>
  Boolean((await lstat(candidate).catch(() => null))?.isDirectory());
const fileExists = async (candidate: string) =>
  Boolean((await lstat(candidate).catch(() => null))?.isFile());

/**
 * Maps a 0.10.x mode token onto a public profile. This is the only reader of
 * `base`/`full` anywhere in the product; the public CLI does not accept them.
 */
export function legacyModeToken(value: string): Profile | null {
  if (value === "base") return "vault";
  if (value === "full") return "akasha";
  return null;
}

async function collectDotenvAndBackups(root: string, preserve: PreservedItem[], seen: Set<string>): Promise<void> {
  const dotenv = path.join(root, ".env");
  if (await fileExists(dotenv) && !seen.has(dotenv)) {
    seen.add(dotenv);
    preserve.push({ kind: "dotenv", source: dotenv, destination: path.join("state", "substrate", ".env") });
  }
  const backups = path.join(root, "backups");
  if (await directoryExists(backups) && !seen.has(backups)) {
    seen.add(backups);
    preserve.push({ kind: "backups", source: backups, destination: path.join("state", "substrate", "backups") });
  }
}

/**
 * Recognise a 0.10.x installation at `target`.
 *
 * `configText` is the operator's OMP config as read from disk, `env` the
 * process environment. Both contribute detection signals of their own: an
 * install root may already have been half-cleaned while the config still points
 * at `solarisael-house-omp`.
 */
export async function detectLegacyInstall(options: {
  target: string;
  configText: string;
  env: NodeJS.ProcessEnv;
  substrate?: string;
}): Promise<LegacyDetection> {
  const reasons: string[] = [];
  const productDirectories: string[] = [];
  const preserve: PreservedItem[] = [];
  const seen = new Set<string>();
  let substrateDirectory: string | null = null;

  for (const name of LEGACY_PRODUCT_DIRECTORIES) {
    const candidate = path.join(options.target, name);
    if (!(await directoryExists(candidate))) continue;
    productDirectories.push(candidate);
    reasons.push(`0.10.x product directory: ${candidate}`);
    if (name === "solarisael-house-substrate") substrateDirectory = candidate;
    await collectDotenvAndBackups(candidate, preserve, seen);
    for (const manifest of ["package-manifest.json", "rust-manifest.json"]) {
      const file = path.join(candidate, manifest);
      if (!(await fileExists(file))) continue;
      preserve.push({
        kind: "package-manifest",
        source: file,
        destination: path.join("state", "legacy", name, manifest),
      });
    }
  }

  if (options.substrate) {
    const explicit = path.resolve(options.substrate);
    if (await directoryExists(explicit)) {
      substrateDirectory = substrateDirectory || explicit;
      await collectDotenvAndBackups(explicit, preserve, seen);
    }
  }

  // The 0.10.x installer already wrote rooms to <target>/rooms; the unified
  // layout keeps that path, but the migration copies them explicitly rather
  // than relying on a whole-tree merge that would drag product files along.
  const rooms = path.join(options.target, "rooms");
  const hasRooms = await directoryExists(rooms);
  if (hasRooms) preserve.push({ kind: "rooms", source: rooms, destination: "rooms", overwrite: true });

  await collectDotenvAndBackups(options.target, preserve, seen);

  const staleExtensionPaths = options.configText
    .split(/\r?\n/)
    .map((line) => line.trim().replace(/^-\s*/, "").replace(/^(['"])(.*)\1$/, "$2"))
    .filter((line) => EXTENSION_PATH_PATTERN.test(line));
  const legacyConfigured = staleExtensionPaths.filter((value) => /solarisael-house-omp/i.test(value));
  if (legacyConfigured.length) {
    reasons.push(`OMP config points at 0.10.x extensions: ${legacyConfigured.join(", ")}`);
  }

  const topologyVariables = LEGACY_TOPOLOGY_VARIABLES
    .filter((name) => String(options.env[name] ?? "").trim() !== "");
  if (topologyVariables.length) {
    reasons.push(`0.10.x topology variables set: ${topologyVariables.join(", ")}`);
  }

  return {
    detected: reasons.length > 0,
    requiresMigration: productDirectories.length > 0,
    reasons,
    productDirectories,
    rooms: hasRooms ? rooms : null,
    substrateDirectory,
    preserve,
    staleExtensionPaths,
    topologyVariables: [...topologyVariables],
  };
}

export type BackupProof = {
  path: string;
  size: number;
  modifiedAt: string;
};

const PGDMP_MAGIC = Buffer.from("PGDMP", "ascii");

/** A custom-format pg_dump starts with the literal bytes `PGDMP`. */
async function looksLikePostgresDump(file: string): Promise<boolean> {
  const handle = await open(file, "r").catch(() => null);
  if (!handle) return false;
  try {
    const buffer = Buffer.alloc(PGDMP_MAGIC.byteLength);
    const { bytesRead } = await handle.read(buffer, 0, buffer.byteLength, 0);
    return bytesRead === buffer.byteLength && buffer.equals(PGDMP_MAGIC);
  } finally {
    await handle.close();
  }
}

async function newestDump(directory: string): Promise<string | null> {
  const entries = await readdir(directory, { withFileTypes: true }).catch(() => null);
  if (!entries) return null;
  let newest: { file: string; time: number } | null = null;
  for (const entry of entries) {
    if (!entry.isFile() || !entry.name.endsWith(".dump")) continue;
    const file = path.join(directory, entry.name);
    const time = (await stat(file)).mtimeMs;
    if (!newest || time > newest.time) newest = { file, time };
  }
  return newest?.file ?? null;
}

/**
 * The exact command to take the backup, phrased so it exists RIGHT NOW.
 *
 * Before migration there is no `the-athanor/substrate/` — pointing at it would
 * send the operator to a path the installer has not created yet. So: prefer the
 * detected 0.10.x `backup.sh` if it is really on disk, otherwise give a direct
 * `pg_dump` invocation that depends on nothing this installer has or will
 * install. Defaults mirror substrate/backup.sh.
 */
export async function backupInstruction(detection: LegacyDetection, destination: string): Promise<string> {
  const candidates = [
    detection.substrateDirectory ? path.join(detection.substrateDirectory, "backup.sh") : null,
    ...detection.productDirectories.map((directory) => path.join(directory, "backup.sh")),
  ].filter((value): value is string => Boolean(value));
  for (const script of candidates) {
    if (await fileExists(script)) return `bash ${script}`;
  }
  return `pg_dump -h 127.0.0.1 -p 5432 -U solarisael -d solarisael_memory -Fc --no-owner --no-acl -f ${path.join(destination, "solarisael_memory.dump")}`;
}

/**
 * Refuse an AKASHA migration without proof of a recent PostgreSQL dump.
 *
 * The installer never takes the backup itself: taking one needs a live
 * database, `pg_dump` on PATH, and on Windows a WSL shell, and a silent
 * half-successful dump is worse than no dump. So the operator runs it, and this
 * function proves the artifact — real custom-format dump, non-empty, recent.
 */
export async function requireDatabaseBackup(options: {
  explicit?: string;
  searchDirectories: string[];
  maxAgeHours: number;
  /** A command that exists before migration. See {@link backupInstruction}. */
  backupCommand: string;
  now?: number;
}): Promise<BackupProof> {
  const now = options.now ?? Date.now();
  const candidates: string[] = [];
  if (options.explicit) candidates.push(path.resolve(options.explicit));
  else {
    for (const directory of options.searchDirectories) {
      const dump = await newestDump(directory);
      if (dump) candidates.push(dump);
    }
  }
  if (!candidates.length) {
    throw new Error([
      "AKASHA activation refused: no PostgreSQL backup was found.",
      `Searched: ${options.searchDirectories.join(", ") || "(no backup directory)"}.`,
      `Take one with: ${options.backupCommand}`,
      "then rerun with --backup PATH.",
    ].join(" "));
  }

  let newest: BackupProof | null = null;
  for (const candidate of candidates) {
    const details = await stat(candidate).catch(() => null);
    if (!details?.isFile()) {
      if (options.explicit) throw new Error(`AKASHA activation refused: --backup is not a regular file: ${candidate}`);
      continue;
    }
    if (details.size === 0) {
      if (options.explicit) throw new Error(`AKASHA activation refused: backup is empty: ${candidate}`);
      continue;
    }
    if (!(await looksLikePostgresDump(candidate))) {
      throw new Error(`AKASHA activation refused: ${candidate} is not a custom-format pg_dump archive (missing PGDMP header).`);
    }
    if (!newest || details.mtimeMs > Date.parse(newest.modifiedAt)) {
      newest = { path: candidate, size: details.size, modifiedAt: new Date(details.mtimeMs).toISOString() };
    }
  }
  if (!newest) throw new Error("AKASHA activation refused: no usable PostgreSQL backup was found.");

  const ageHours = (now - Date.parse(newest.modifiedAt)) / 3_600_000;
  if (ageHours > options.maxAgeHours) {
    throw new Error([
      `AKASHA activation refused: the newest backup ${newest.path} is ${ageHours.toFixed(1)}h old,`,
      `older than the ${options.maxAgeHours}h freshness window.`,
      `Take a fresh dump with: ${options.backupCommand}`,
      "or widen --backup-max-age-hours deliberately.",
    ].join(" "));
  }
  return newest;
}

/** Directories a migration should search for an existing dump. */
export function backupSearchDirectories(target: string, detection: LegacyDetection): string[] {
  const directories = new Set<string>([layout(target).substrateBackups]);
  for (const item of detection.preserve) {
    if (item.kind === "backups") directories.add(item.source);
  }
  if (detection.substrateDirectory) directories.add(path.join(detection.substrateDirectory, "backups"));
  return [...directories];
}

/** Read the OMP config without failing when it does not exist yet. */
export async function readConfigText(configPath: string): Promise<string> {
  return await readFile(configPath, "utf8").catch(() => "");
}

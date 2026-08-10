// The one description of the installed Athanor topology.
//
// Every installation door — the portable builder, the installer, the updater,
// the release-manifest writer, and the verifier — reads the layout from here so
// that a path can never mean two things in two files.
//
//   <root>/the-athanor            immutable product tree (never written after install)
//   <root>/the-athanor/adapters/omp
//   <root>/the-athanor/substrate  substrate operations (AKASHA only)
//   <root>/rooms                  rooms, owned by the operator
//   <root>/state/substrate        mutable substrate state: dotenv, backups
//
// The 0.10.x layout this replaces put three sibling product directories at the
// install root. Those names live on in LEGACY_PRODUCT_DIRECTORIES purely so the
// migration can recognise and retire them; nothing else may consult them.

import path from "node:path";

/** The two public installation profiles. */
export const PROFILES = ["vault", "akasha"] as const;
export type Profile = (typeof PROFILES)[number];

export function isProfile(value: unknown): value is Profile {
  return typeof value === "string" && (PROFILES as readonly string[]).includes(value);
}

/** The immutable product directory name inside an install root. */
export const PRODUCT_DIRECTORY = "the-athanor";
/** The adapter's position inside the product tree, POSIX-joined for archive entries. */
export const ADAPTER_RELATIVE = `${PRODUCT_DIRECTORY}/adapters/omp`;
/** Substrate operations inside the product tree. AKASHA profile only. */
export const SUBSTRATE_RELATIVE = `${PRODUCT_DIRECTORY}/substrate`;
/** Operator-owned rooms, deliberately outside the immutable product tree. */
export const ROOMS_DIRECTORY = "rooms";
/** Mutable state root. */
export const STATE_DIRECTORY = "state";
/** Mutable substrate state: `.env` and PostgreSQL dumps. */
export const SUBSTRATE_STATE_RELATIVE = `${STATE_DIRECTORY}/substrate`;

/**
 * The runtime environment file. It lives in mutable state, not in the immutable
 * product tree, and it is the contract an installed adapter reads structurally
 * so a fresh shell needs no exported topology.
 */
export const ENVIRONMENT_FILE = `${STATE_DIRECTORY}/athanor.env`;
/** 0.10.x sibling product directories. Only the legacy migration may use these. */
export const LEGACY_PRODUCT_DIRECTORIES = [
  "solarisael-house",
  "solarisael-house-omp",
  "solarisael-house-substrate",
] as const;

/** 0.10.x topology variables. Only the legacy migration may use these. */
export const LEGACY_TOPOLOGY_VARIABLES = [
  "SOLARISAEL_HOUSE_CORE",
  "SOLARISAEL_HOUSE_RUST",
  "SOLARISAEL_HOUSE_RUST_AUTO",
  "SOLARISAEL_HOUSE_AUTO",
  "SOLARISAEL_SUBSTRATE",
  "SOLARISAEL_STATE_DIR",
] as const;

/** The canonical topology variables a current install emits. */
export const TOPOLOGY_VARIABLES = [
  "ATHANOR_STATE_DIR",
  "ATHANOR_SUBSTRATE_ROOT",
  "ATHANOR_SUBSTRATE_EXE",
] as const;

/**
 * Matches both the 0.10.x extension paths and the current ones, so a config
 * rewrite can strip every generation before adding the canonical pair. A path
 * that only *contains* the old product name is caught by the first alternative.
 */
export const EXTENSION_PATH_PATTERN =
  /(?:solarisael-house-omp|adapters[\\/]omp)[\\/](?:index|hygiene)\.ts\s*$/i;

/** Absolute paths inside an install root. */
export function layout(root: string) {
  const product = path.join(root, PRODUCT_DIRECTORY);
  const state = path.join(root, STATE_DIRECTORY);
  return {
    root,
    product,
    adapter: path.join(product, "adapters", "omp"),
    substrate: path.join(product, "substrate"),
    rooms: path.join(root, ROOMS_DIRECTORY),
    state,
    substrateState: path.join(state, "substrate"),
    substrateDotenv: path.join(state, "substrate", ".env"),
    substrateBackups: path.join(state, "substrate", "backups"),
    environmentFile: path.join(state, "athanor.env"),
  };
}

/**
 * One comparison form for a path that may have been reported from inside WSL.
 *
 * health.py runs through `wsl.exe`, so it answers with `/mnt/c/Users/...` while
 * the installer and verifier hold `C:\Users\...`. Those name the same directory
 * and must not compare as drift. This ONLY rewrites the drive prefix and slash
 * direction — it never resolves, never touches the filesystem, and never
 * relaxes a containment or existence check, which still run on real paths.
 */
export function comparablePath(value: string): string {
  const text = String(value ?? "").trim().replaceAll("\\", "/");
  const mount = /^\/mnt\/([A-Za-z])(?=\/|$)(.*)$/i.exec(text);
  const windows = mount ? `${(mount[1] as string).toUpperCase()}:${mount[2] || "/"}` : text;
  return windows.replace(/\/+$/, "").toLowerCase();
}

/** The two OMP extension entrypoints an install wires, in configuration order. */
export function extensionPaths(root: string): string[] {
  const adapter = layout(root).adapter;
  return [path.join(adapter, "index.ts"), path.join(adapter, "hygiene.ts")]
    .map((value) => value.replaceAll("\\", "/"));
}

export type ArchivePlatform = "windows-x64" | "linux-x64" | "linux-arm64";

export const ARCHIVE_PLATFORMS: readonly ArchivePlatform[] = ["windows-x64", "linux-x64", "linux-arm64"];

export function archivePlatform(
  platform: NodeJS.Platform = process.platform,
  arch: string = process.arch,
): ArchivePlatform | null {
  if (platform === "win32" && arch === "x64") return "windows-x64";
  if (platform === "linux" && arch === "x64") return "linux-x64";
  if (platform === "linux" && arch === "arm64") return "linux-arm64";
  return null;
}

/** `the-athanor-<version>-<platform>-<profile>.zip` — the only public archive name. */
export function archiveName(version: string, platform: ArchivePlatform, profile: Profile): string {
  return `the-athanor-${version}-${platform}-${profile}.zip`;
}

const ARCHIVE_NAME_PATTERN =
  /^the-athanor-(\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?)-(windows-x64|linux-x64|linux-arm64)-(vault|akasha)\.zip$/;

/** Inverse of {@link archiveName}. Returns null for anything not canonically named. */
export function parseArchiveName(
  name: string,
): { version: string; platform: ArchivePlatform; profile: Profile } | null {
  const match = ARCHIVE_NAME_PATTERN.exec(name);
  if (!match) return null;
  return {
    version: match[1] as string,
    platform: match[2] as ArchivePlatform,
    profile: match[3] as Profile,
  };
}

export function substrateBinaryName(platform: ArchivePlatform): string {
  return platform === "windows-x64" ? "athanor-substrate.exe" : "athanor-substrate";
}

/** `bin/<platform>/athanor-substrate[.exe]`, relative to the adapter directory. */
export function substrateBinaryRelative(platform: ArchivePlatform): string {
  return `bin/${platform}/${substrateBinaryName(platform)}`;
}

/** Archive entries every profile must carry. */
export const REQUIRED_ENTRIES: readonly string[] = [
  `${PRODUCT_DIRECTORY}/index.ts`,
  `${PRODUCT_DIRECTORY}/package.json`,
  `${ADAPTER_RELATIVE}/index.ts`,
  `${ADAPTER_RELATIVE}/hygiene.ts`,
  `${ADAPTER_RELATIVE}/discovery.ts`,
  `${ADAPTER_RELATIVE}/harnesses.ts`,
  `${ADAPTER_RELATIVE}/athanor-root.ts`,
  `${ADAPTER_RELATIVE}/install-layout.ts`,
  `${ADAPTER_RELATIVE}/verify-install.ts`,
  `${ADAPTER_RELATIVE}/package-manifest.json`,
  `${ADAPTER_RELATIVE}/starter-room/example/.solarisael-room.json`,
  `${ADAPTER_RELATIVE}/starter-room/example/active_spirit.md`,
  `${ADAPTER_RELATIVE}/starter-room/example/AGENTS.md`,
];

/** Archive entries only the AKASHA profile carries. */
export function akashaRequiredEntries(platform: ArchivePlatform): string[] {
  return [
    `${SUBSTRATE_RELATIVE}/health.py`,
    `${SUBSTRATE_RELATIVE}/compatibility.json`,
    `${SUBSTRATE_RELATIVE}/state_paths.py`,
    `${SUBSTRATE_RELATIVE}/backup.sh`,
    `${ADAPTER_RELATIVE}/rust-manifest.json`,
    `${ADAPTER_RELATIVE}/${substrateBinaryRelative(platform)}`,
  ];
}

/**
 * Entry prefixes the Vault profile must never contain. Vault carries no
 * substrate binary, no substrate operations, and no PostgreSQL or Rust runtime
 * assets of any kind.
 */
export const VAULT_FORBIDDEN_PREFIXES: readonly string[] = [
  `${SUBSTRATE_RELATIVE}/`,
  `${ADAPTER_RELATIVE}/bin/`,
];

/** Entries the Vault profile must never contain, exactly. */
export const VAULT_FORBIDDEN_ENTRIES: readonly string[] = [
  `${ADAPTER_RELATIVE}/rust-manifest.json`,
];

/**
 * Markers that make an installed tree look like a development checkout.
 *
 * The substrate's Python and bash state resolvers identify a development
 * checkout by `Cargo.toml` AND `crates/` both being present next to the product
 * root. If either ever shipped inside a bundle, an installed tree could resolve
 * its state root through the development fallback instead of
 * `<install-root>/state` — writing runtime state somewhere nobody looks. No
 * profile ships them, and the installer refuses a bundle that carries one.
 */
export const DEVELOPMENT_MARKER_ENTRIES: readonly string[] = [
  `${PRODUCT_DIRECTORY}/Cargo.toml`,
  `${PRODUCT_DIRECTORY}/Cargo.lock`,
];

export const DEVELOPMENT_MARKER_PREFIXES: readonly string[] = [
  `${PRODUCT_DIRECTORY}/crates/`,
  `${PRODUCT_DIRECTORY}/target/`,
];

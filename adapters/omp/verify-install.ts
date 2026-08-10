import { createHash } from "node:crypto";
import { existsSync, readFileSync, realpathSync, statSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { pathToFileURL } from "node:url";
import { currentRustPlatform, discoverRustExecutable, rustBinaryName } from "./discovery.ts";
import { substrateHealth } from "./solarisael-house-proof/substrate.ts";
import { ADAPTER_ROOT, ATHANOR_ROOT } from "./athanor-root.ts";
import {
  LEGACY_PRODUCT_DIRECTORIES,
  comparablePath,
  PRODUCT_DIRECTORY,
  archivePlatform,
  isProfile,
  layout,
  substrateBinaryRelative,
  type Profile,
} from "./install-layout.ts";

// The substrate compatibility.json contract this adapter is built against.
// Bump together with substrate/compatibility.json when a migration lands.
const COMPATIBILITY_FORMAT = 1;
const COMPATIBILITY_SCHEMA_VERSION = 13;

type Diagnostic = {
  category: string;
  stage: string;
  operation: string;
  owner: { component: string; path: string; symbol: string };
  expected: Record<string, unknown>;
  observed: Record<string, unknown>;
  evidence: Array<Record<string, unknown>>;
  targets: Array<Record<string, unknown>>;
  next_checks: Array<Record<string, unknown>>;
  execution: { request_dispatched: boolean; write_outcome: "not_started"; retry: "safe_now" | "after_change" };
};

type Check = {
  name: string;
  ok: boolean;
  detail: string;
  diagnostic?: Diagnostic;
};

type CompatibilityContract = Record<string, unknown>;
type VerificationMode = "Vault" | "AKASHA" | "degraded";
type PortableRelease = {
  productVersion: string | null;
  supportedHarnesses: string[];
  requiredSchemaVersion: number | null;
  profile: Profile | null;
};


function argument(name: string) {
  const index = process.argv.indexOf(name);
  return index >= 0 ? process.argv[index + 1] : null;
}

function normalized(value: string) {
  return path.resolve(value).replaceAll("\\", "/").toLowerCase();
}

function canonical(value: string) {
  try {
    return normalized(realpathSync(value));
  } catch {
    return normalized(value);
  }
}

function absolutePath(value: string) {
  const source = String(value || "").trim();
  return path.posix.isAbsolute(source)
    || path.win32.isAbsolute(source)
    || /^[A-Za-z]:[\\/]/.test(source)
    || /^\\\\/.test(source);
}

function validRoomKey(value: unknown) {
  return typeof value === "string"
    && value !== "house"
    && /^[a-z0-9]+(?:-[a-z0-9]+)*$/.test(value);
}

function validDisplayName(value: unknown) {
  return typeof value === "string"
    && value.trim().length > 0
    && value.trim().length <= 80
    && !/[\r\n|]/.test(value);
}
function add(checks: Check[], name: string, ok: boolean, detail: string, diagnostic?: Diagnostic) {
  checks.push({ name, ok, detail, diagnostic });
}

function redacted(value: unknown) {
  return String(value ?? "")
    .replace(/([a-z][a-z0-9+.-]*:\/\/)[^/\s@]+@/gi, "$1[redacted]@")
    .replace(/\b[\w.-]+:[^@\\/\s]+@/g, "[redacted]@")
    .replace(/\b(token|password|secret|api[_-]?key|authorization)\s*[:=]\s*\S+/gi, "$1: [redacted]");
}

function checkDiagnostic(check: Check): Diagnostic {
  const lower = check.name.toLowerCase();
  const category = /rust/.test(lower) ? "configuration"
    : /compatibility|api|schema/.test(lower) ? "protocol"
    : /substrate|config/.test(lower) ? "configuration"
    : "operation";
  const target = /rust/.test(lower)
    ? { kind: "environment", name: "ATHANOR_SUBSTRATE_EXE" }
    : /config/.test(lower)
      ? { kind: "file", path: configPath }
      : /compatibility|api|schema/.test(lower)
        ? { kind: "file", path: contractPath || "compatibility.json" }
        : { kind: "source", path: "verify-install.ts", symbol: check.name };
  return {
    category,
    stage: category === "protocol" ? "validation" : "configuration_load",
    operation: "verify_install",
    owner: { component: "the-athanor-omp", path: "verify-install.ts", symbol: "main" },
    expected: { check: check.name, ok: true },
    observed: { check: check.name, ok: false, detail: redacted(check.detail) },
    evidence: [{ source: "verify-install.ts", check: check.name, detail: redacted(check.detail) }],
    targets: [target],
    next_checks: [
      { action: category === "protocol" ? "validate_compatibility_contract" : "inspect_configuration", target },
      { action: "rerun_verify_install", target: { path: "verify-install.ts" } },
    ],
    execution: { request_dispatched: false, write_outcome: "not_started", retry: "after_change" },
  };
}

function readJson(filePath: string): CompatibilityContract {
  return JSON.parse(readFileSync(filePath, "utf8")) as CompatibilityContract;
}

function verifyRustBundle(checks: Check[], requiredManifest: boolean): void {
  const rustRequested = Boolean(String(process.env.ATHANOR_SUBSTRATE_EXE || "").trim())
    || process.env.ATHANOR_AUTO === "1";
  if (rustRequested) {
    try {
      const executable = discoverRustExecutable({ moduleDir: ADAPTER_ROOT });
      add(checks, "Rust executable selection", Boolean(executable), executable || "no executable was found for the requested Rust selection");
    } catch (error) {
      add(checks, "Rust executable selection", false, error instanceof Error ? error.message : String(error));
    }
  }
  const manifestPath = path.join(ADAPTER_ROOT, "rust-manifest.json");
  if (!existsSync(manifestPath)) {
    if (requiredManifest) add(checks, "Rust manifest", false, manifestPath);
    return;
  }
  let manifest: any;
  try { manifest = readJson(manifestPath); } catch (error) {
    add(checks, "Rust manifest", false, `invalid JSON: ${error instanceof Error ? error.message : String(error)}`);
    return;
  }
  const platform = currentRustPlatform();
  const artifact = Array.isArray(manifest.artifacts) ? manifest.artifacts.find((entry: any) => entry?.platform === platform) : null;
  add(checks, "Rust manifest platform", Boolean(artifact), platform ? `expected ${platform}` : "unsupported host platform");
  if (!artifact || typeof artifact.path !== "string") return;
  const artifactPath = path.resolve(ADAPTER_ROOT, artifact.path);
  const insideAdapter = artifactPath === ADAPTER_ROOT || artifactPath.startsWith(`${ADAPTER_ROOT}${path.sep}`);
  add(checks, "Rust artifact path", insideAdapter, artifactPath);
  if (!insideAdapter || !existsSync(artifactPath)) {
    add(checks, "Rust artifact file", false, artifactPath);
    return;
  }
  try {
    const details = statSync(artifactPath);
    add(checks, "Rust artifact regular file", details.isFile(), artifactPath);
    if (!details.isFile()) return;
    add(checks, "Rust artifact permissions", process.platform === "win32" || (details.mode & 0o111) !== 0, "must be executable");
    const hash = createHash("sha256").update(readFileSync(artifactPath)).digest("hex");
    add(checks, "Rust artifact SHA256", hash === artifact.sha256, `expected ${artifact.sha256}; got ${hash}`);
    add(checks, "Rust artifact size", details.size === artifact.size, `expected ${artifact.size}; got ${details.size}`);
    add(checks, "Rust artifact name", path.basename(artifactPath) === rustBinaryName(platform), path.basename(artifactPath));
  } catch (error) {
    add(checks, "Rust artifact readable", false, error instanceof Error ? error.message : String(error));
  }
}
function verifyPortableManifest(checks: Check[], requiredManifest: boolean, profile: Profile): PortableRelease {
  const empty: PortableRelease = { productVersion: null, supportedHarnesses: [], requiredSchemaVersion: null, profile: null };
  const manifestPath = path.join(ADAPTER_ROOT, "package-manifest.json");
  if (!existsSync(manifestPath)) {
    if (requiredManifest) add(checks, "package manifest", false, manifestPath);
    return empty;
  }
  let manifest: any;
  try { manifest = readJson(manifestPath); } catch (error) {
    add(checks, "package manifest", false, error instanceof Error ? error.message : String(error));
    return empty;
  }
  const productVersion = typeof manifest.productVersion === "string" && /^\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$/.test(manifest.productVersion)
    ? manifest.productVersion
    : null;
  const supportedHarnesses = Array.isArray(manifest.supportedHarnesses)
    && manifest.supportedHarnesses.every((value: unknown) => typeof value === "string")
    ? manifest.supportedHarnesses as string[]
    : [];
  const requiredSchemaVersion = Number.isSafeInteger(manifest.requiredSchemaVersion) && manifest.requiredSchemaVersion >= 0
    ? manifest.requiredSchemaVersion as number
    : null;
  const manifestProfile = isProfile(manifest.profile) ? manifest.profile : null;
  add(checks, "package manifest schema", manifest.schemaVersion === 3, `expected 3, got ${String(manifest.schemaVersion)}`);
  add(checks, "package product version", Boolean(productVersion), String(manifest.productVersion));
  add(checks, "package profile", manifestProfile === profile, `expected ${profile}, got ${String(manifest.profile)}`);
  add(checks, "package platform", manifest.platform === archivePlatform(), `expected ${String(archivePlatform())}, got ${String(manifest.platform)}`);
  add(checks, "package harness catalog", supportedHarnesses.includes("omp"), supportedHarnesses.join(", "));
  add(checks, "package substrate schema requirement", requiredSchemaVersion !== null, String(manifest.requiredSchemaVersion));
  const release = { productVersion, supportedHarnesses, requiredSchemaVersion, profile: manifestProfile };
  if (!Array.isArray(manifest.artifacts) || manifest.artifacts.length === 0) {
    add(checks, "package artifacts", false, manifestPath);
    return release;
  }
  // Manifest artifact paths are relative to the install root, which is three
  // levels above the adapter: <root>/the-athanor/adapters/omp.
  const installRoot = path.resolve(ADAPTER_ROOT, "..", "..", "..");
  for (const artifact of manifest.artifacts) {
    const relative = typeof artifact?.path === "string" ? artifact.path.replaceAll("\\", "/") : "";
    const safe = relative !== "" && !relative.startsWith("/") && !/^[A-Za-z]:/.test(relative) && !relative.split("/").includes("..");
    const file = path.resolve(installRoot, relative);
    const insideInstall = file === installRoot || file.startsWith(`${installRoot}${path.sep}`);
    if (!safe || !insideInstall || !existsSync(file)) {
      add(checks, `portable artifact ${relative || "<invalid>"}`, false, file);
      continue;
    }
    const details = statSync(file);
    const hash = details.isFile() ? createHash("sha256").update(readFileSync(file)).digest("hex") : "";
    const valid = details.isFile()
      && Number.isSafeInteger(artifact.size)
      && details.size === artifact.size
      && typeof artifact.sha256 === "string"
      && hash === artifact.sha256;
    add(checks, `portable artifact ${relative}`, valid, file);
  }
  for (const [label, name] of [["installer", manifest.installer], ["updater", manifest.updater]] as const) {
    const executable = typeof name === "string" ? path.join(ADAPTER_ROOT, name) : "";
    const expected = process.platform === "win32" ? `${label === "installer" ? "install" : "update"}.exe` : label === "installer" ? "install" : "update";
    add(checks, `compiled ${label}`, Boolean(executable && existsSync(executable) && statSync(executable).isFile()), executable || "missing");
    add(checks, `compiled ${label} platform name`, path.basename(executable) === expected, path.basename(executable));
  }
  return release;
}

/**
 * The installed tree must be exactly one product directory beside rooms and
 * state. A surviving 0.10.x sibling means a migration did not finish, and the
 * adapter would then be reachable by two different paths.
 */
function verifyInstalledTopology(checks: Check[], profile: Profile): void {
  const installRoot = path.resolve(ADAPTER_ROOT, "..", "..", "..");
  const paths = layout(installRoot);
  add(checks, "product directory", path.basename(ATHANOR_ROOT) === PRODUCT_DIRECTORY && ATHANOR_ROOT === paths.product, `expected ${paths.product}, got ${ATHANOR_ROOT}`);
  add(checks, "adapter position", ADAPTER_ROOT === paths.adapter, `expected ${paths.adapter}`);
  const legacySiblings = LEGACY_PRODUCT_DIRECTORIES
    .map((name) => path.join(installRoot, name))
    .filter((candidate) => existsSync(candidate));
  add(checks, "no legacy product directories", legacySiblings.length === 0, legacySiblings.join(", ") || installRoot);
  add(checks, "rooms directory", existsSync(paths.rooms), paths.rooms);

  const stateDirectory = String(process.env.ATHANOR_STATE_DIR || "").trim();
  add(checks, "ATHANOR_STATE_DIR", Boolean(stateDirectory) && comparablePath(stateDirectory) === comparablePath(paths.state), `expected ${paths.state}, got ${stateDirectory || "<unset>"}`);
  if (profile !== "akasha") return;

  add(checks, "substrate state directory", existsSync(paths.substrateState), paths.substrateState);
  const platform = archivePlatform();
  const executable = String(process.env.ATHANOR_SUBSTRATE_EXE || "").trim();
  const expectedExecutable = platform ? path.join(paths.adapter, substrateBinaryRelative(platform)) : null;
  add(
    checks,
    "ATHANOR_SUBSTRATE_EXE",
    Boolean(expectedExecutable && executable && comparablePath(executable) === comparablePath(expectedExecutable)),
    `expected ${expectedExecutable || "<unsupported platform>"}, got ${executable || "<unset>"}`,
  );
}


const roomArgument = argument("--room");
const configPath = path.resolve(argument("--config") || path.join(os.homedir(), ".omp", "agent", "config.yml"));

// The profile is the one switch. Vault verifies statically and must not touch a
// substrate; AKASHA resolves its substrate structurally from the product tree
// and may still be overridden by ATHANOR_SUBSTRATE_ROOT for a staged check.
const profileArgument = String(argument("--profile") || "").trim();
if (profileArgument && !isProfile(profileArgument)) {
  throw new Error(`--profile must be vault or akasha (got ${profileArgument})`);
}
const configuredSubstrate = String(process.env.ATHANOR_SUBSTRATE_ROOT || "").trim() || null;
const profile: Profile = isProfile(profileArgument)
  ? profileArgument
  : configuredSubstrate ? "akasha" : "vault";
const substrateSetting = profile === "akasha"
  ? configuredSubstrate || path.join(ATHANOR_ROOT, "substrate")
  : null;
const substrateConfigured = Boolean(substrateSetting);
const substrateAbsolute = !substrateConfigured || absolutePath(substrateSetting as string);
const substrateRoot = substrateConfigured && substrateAbsolute ? path.resolve(substrateSetting as string) : null;
const substratePathError = substrateConfigured && !substrateAbsolute
  ? `ATHANOR_SUBSTRATE_ROOT must be an absolute path when configured (got ${substrateSetting})`
  : null;
const contractPath = substrateRoot ? path.join(substrateRoot, "compatibility.json") : null;
if (substrateRoot) process.env.ATHANOR_SUBSTRATE_ROOT = substrateRoot;
else delete process.env.ATHANOR_SUBSTRATE_ROOT;
const checks: Check[] = [];
let compatibilityContract: CompatibilityContract | null = null;
let compatibleApis = false;
let adapterApiVersion: unknown = null;
let coreApiVersion: unknown = null;
let rootImportError: string | null = null;

try {
  const adapterPackage = await import(pathToFileURL(path.join(ADAPTER_ROOT, "index.ts")).href);
  adapterApiVersion = adapterPackage.ADAPTER_API_VERSION;
} catch (error) {
  rootImportError = `adapter root import failed: ${error instanceof Error ? error.message : String(error)}`;
}
try {
  const corePackage = await import(pathToFileURL(path.join(ATHANOR_ROOT, "index.ts")).href);
  coreApiVersion = corePackage.CORE_API_VERSION;
} catch (error) {
  rootImportError = rootImportError
    ? `${rootImportError}; core root import failed: ${error instanceof Error ? error.message : String(error)}`
    : `core root import failed: ${error instanceof Error ? error.message : String(error)}`;
}

let runtimeHealth: {
  ok: boolean | null;
  state: string;
  detail: string;
  verdict: Record<string, unknown> | null;
} = {
  ok: null,
  state: "not-configured",
  detail: "Substrate is not configured.",
  verdict: null,
};

add(checks, "core package", existsSync(path.join(ATHANOR_ROOT, "index.ts")), path.join(ATHANOR_ROOT, "index.ts"));
add(checks, "core API export", coreApiVersion === 1, rootImportError || `expected 1, got ${String(coreApiVersion)}`);
add(checks, "OMP adapter entrypoint", existsSync(path.join(ADAPTER_ROOT, "index.ts")), path.join(ADAPTER_ROOT, "index.ts"));
add(checks, "adapter API export", adapterApiVersion === 1, rootImportError || `expected 1, got ${String(adapterApiVersion)}`);
add(checks, "OMP hygiene extension", existsSync(path.join(ADAPTER_ROOT, "hygiene.ts")), path.join(ADAPTER_ROOT, "hygiene.ts"));

if (substrateConfigured) {
  add(checks, "substrate path absolute", !substratePathError, substratePathError || String(substrateRoot));
  if (substrateRoot) add(checks, "substrate directory", existsSync(substrateRoot), substrateRoot);
  if (!contractPath || !existsSync(contractPath)) {
    add(checks, "compatibility contract JSON", false, contractPath || "missing substrate compatibility.json");
  } else {
    try {
      compatibilityContract = readJson(contractPath);
      add(checks, "compatibility contract JSON", true, contractPath);
    } catch (error) {
      add(checks, "compatibility contract JSON", false, error instanceof Error ? error.message : String(error));
    }
  }
  const schemaOk = compatibilityContract?.format === COMPATIBILITY_FORMAT
    && compatibilityContract?.schemaVersion === COMPATIBILITY_SCHEMA_VERSION;
  add(
    checks,
    "compatibility schema",
    schemaOk,
    `expected format=${COMPATIBILITY_FORMAT} schemaVersion=${COMPATIBILITY_SCHEMA_VERSION}; got format=${String(compatibilityContract?.format)} schemaVersion=${String(compatibilityContract?.schemaVersion)}`,
  );

  const substrateApiOk = compatibilityContract?.substrateApi === 1;
  const coreApiOk = coreApiVersion === 1 && compatibilityContract?.coreApi === coreApiVersion;
  const adapterApiOk = adapterApiVersion === 1 && compatibilityContract?.adapterApi === adapterApiVersion;
  add(checks, "substrate API compatibility", substrateApiOk, `expected 1, got ${String(compatibilityContract?.substrateApi)}`);
  add(checks, "core API compatibility", coreApiOk, `expected ${String(coreApiVersion)}, got ${String(compatibilityContract?.coreApi)}`);
  add(checks, "adapter API compatibility", adapterApiOk, `expected ${String(adapterApiVersion)}, got ${String(compatibilityContract?.adapterApi)}`);
  compatibleApis = schemaOk && substrateApiOk && coreApiOk && adapterApiOk;

  const verdict = await substrateHealth();
  const healthy = verdict.ok === true && verdict.mode === "full" && verdict.substrateApi === 1;
  const detail = healthy
    ? "health.py proved a healthy, compatible substrate."
    : verdict.reason
      || (Array.isArray(verdict.degradedReasons) ? verdict.degradedReasons.join("; ") : "")
      || "health.py reported an unhealthy substrate.";
  runtimeHealth = {
    ok: healthy,
    state: healthy ? "healthy" : "unhealthy",
    detail,
    verdict,
  };
  add(checks, "substrate runtime health", healthy, detail);
}


if (!roomArgument) {
  add(checks, "room argument", false, "Pass --room with the absolute room directory.");
} else {
  const roomDir = path.resolve(roomArgument);
  const markerPath = path.join(roomDir, ".solarisael-room.json");
  const spiritPath = path.join(roomDir, "active_spirit.md");
  const agentsPath = path.join(roomDir, "AGENTS.md");
  add(checks, "room directory", existsSync(roomDir), roomDir);
  add(checks, "room marker", existsSync(markerPath), markerPath);
  add(checks, "active spirit", existsSync(spiritPath), spiritPath);
  add(checks, "host context entrypoint", existsSync(agentsPath), agentsPath);

  let marker: Record<string, unknown> | null = null;
  try {
    marker = JSON.parse(readFileSync(markerPath, "utf8"));
    add(checks, "room marker JSON", true, markerPath);
  } catch (error) {
    add(checks, "room marker JSON", false, error instanceof Error ? error.message : String(error));
  }

  if (marker) {
    const roomKey = String(marker.room || "");
    const folderKey = path.basename(roomDir).toLowerCase();
    add(checks, "room key format", validRoomKey(roomKey), roomKey || "missing marker.room");
    add(checks, "room key reserved", roomKey !== "house", roomKey === "house" ? "room key 'house' is reserved for the House substrate" : "room key is available");
    add(checks, "room key matches folder", roomKey === folderKey, `marker=${roomKey || "missing"}; folder=${folderKey}`);
    add(checks, "true name", validDisplayName(marker.trueName), String(marker.trueName || "missing marker.trueName"));
    add(checks, "operator", validDisplayName(marker.operator), String(marker.operator || "missing marker.operator"));

    if (existsSync(spiritPath)) {
      const spirit = readFileSync(spiritPath, "utf8").replace(/\r\n?/g, "\n");
      const trueName = String(marker.trueName || "");
      const operator = String(marker.operator || "");
      add(checks, "active spirit header", spirit.startsWith(`# Active Spirit: ${trueName}\n`), `expected true name ${trueName || "<missing>"}`);
      add(checks, "agent/operator header", spirit.includes(`Agent: ${trueName} | Operator: ${operator}`), "header must match room marker");
      add(checks, "spirit body", spirit.includes(`# SPIRIT: ${trueName}`), "identity body heading must match true name");
    }
  }

  if (existsSync(agentsPath)) {
    const agents = readFileSync(agentsPath, "utf8");
    add(checks, "active spirit context include", agents.includes("@active_spirit.md"), "AGENTS.md must include @active_spirit.md");
    add(checks, "summary context include", agents.includes("@room_summary.md"), "AGENTS.md must include @room_summary.md");
  }
}

if (!existsSync(configPath)) {
  add(checks, "OMP config", false, configPath);
} else {
  const config = readFileSync(configPath, "utf8").replaceAll("\\", "/").toLowerCase();
  const configuredPaths = config.split(/\r?\n/)
    .map((line) => line.trim().replace(/^-\s*/, "").replace(/^(['"])(.*)\1$/, "$2"))
    .filter(absolutePath)
    .map(canonical);
  const entrypoint = canonical(path.join(ADAPTER_ROOT, "index.ts"));
  const hygiene = canonical(path.join(ADAPTER_ROOT, "hygiene.ts"));
  add(checks, "OMP entrypoint configured", configuredPaths.includes(entrypoint), entrypoint);
  add(checks, "OMP hygiene configured", configuredPaths.includes(hygiene), hygiene);
}
const requireManifest = process.argv.includes("--require-manifest");
verifyRustBundle(checks, requireManifest && profile === "akasha");
const portableRelease = verifyPortableManifest(checks, requireManifest, profile);
// Topology is only meaningful for an installed tree; a development checkout has
// no rooms/ or state/ sibling and is verified without --require-manifest.
if (requireManifest) verifyInstalledTopology(checks, profile);
// An unreachable server and an out-of-date schema are different failures with
// different repairs. Collapsing them reported "required 13; got undefined" and
// blamed the schema for a database nobody ever contacted.
if (substrateConfigured && portableRelease.requiredSchemaVersion !== null) {
  const database = (runtimeHealth.verdict as any)?.database;
  // Same guard shape as "substrate topology reported": a block that vanishes is
  // a reporting defect, and it must not read as a passing check.
  add(checks, "substrate database reported", Boolean(database), database ? "health.py reported its database probe" : "health.py reported no database block");
  if (database?.reachable === false) {
    add(checks, "substrate database reachable", false, redacted(database.error || "health.py could not reach PostgreSQL"));
  } else if (database) {
    add(checks, "substrate database reachable", true, "health.py reached PostgreSQL");
    const schemaVersion = database.schemaVersion;
    add(
      checks,
      "substrate database migration",
      Number.isSafeInteger(schemaVersion) && schemaVersion >= portableRelease.requiredSchemaVersion,
      `required ${portableRelease.requiredSchemaVersion}; got ${String(schemaVersion)}`,
    );
  }
}
// health.py reports how IT resolved the substrate topology. An installed tree
// must never be resolving state through the development-checkout fallback: that
// means the process is writing somewhere other than <install-root>/state, which
// no static path check on this side can see.
if (requireManifest && profile === "akasha") {
  const topology = (runtimeHealth.verdict as any)?.topology;
  add(checks, "substrate topology reported", Boolean(topology), topology ? "health.py reported its resolved topology" : "health.py reported no topology block");
  if (topology) {
    const source = String(topology.stateRootSource || "");
    // Denylist, not allowlist. The set of legitimate resolution sources grows
    // as the substrate gains ways to be told where state lives (environment,
    // installed_tree, explicit_env_file, ...), and an allowlist would turn each
    // addition into a false red on a correct install. Exactly one source is
    // wrong for an installed tree, and the path agreement check below is the
    // real guard: it compares values, not vocabulary.
    add(
      checks,
      "substrate state resolution",
      source !== "" && source !== "development_checkout",
      source === "development_checkout"
        ? "an installed tree must not resolve state through the development-checkout fallback"
        : `reported ${source || "<missing>"}`,
    );
    // health.py answers from inside WSL, so its paths arrive as /mnt/c/... .
    // comparablePath folds those onto the Windows form; containment and
    // existence checks elsewhere still run against real, unfolded paths.
    const expectedState = layout(path.resolve(ADAPTER_ROOT, "..", "..", "..")).state;
    add(
      checks,
      "substrate state root agreement",
      typeof topology.stateRoot === "string" && comparablePath(topology.stateRoot) === comparablePath(expectedState),
      `expected ${expectedState}; got ${String(topology.stateRoot)}`,
    );
    add(checks, "substrate executable resolved", topology.executableFound === true, String(topology.error || topology.executable || "health.py found no substrate executable"));
    // Only meaningful once health.py names the binary it actually resolved.
    if (typeof topology.executable === "string" && topology.executable !== "") {
      const platform = archivePlatform();
      const expectedExecutable = platform
        ? path.join(layout(path.resolve(ADAPTER_ROOT, "..", "..", "..")).adapter, substrateBinaryRelative(platform))
        : null;
      add(
        checks,
        "substrate executable agreement",
        Boolean(expectedExecutable) && comparablePath(topology.executable) === comparablePath(expectedExecutable as string),
        `expected ${expectedExecutable || "<unsupported platform>"}; got ${topology.executable}`,
      );
    }
  }
}

// The installer seeds a proven dump into <install-root>/state/substrate/backups.
// health.py resolves its own backup directory independently, so comparing the
// two is the only way to catch the safety net being written somewhere the
// substrate never looks — the same drift class as stateRootSource, one layer in.
// The age bound stays health.py's policy: a dump ageing after install is an
// operational alarm, not an installation defect, so it is reported, not failed.
if (requireManifest && profile === "akasha") {
  const backup = (runtimeHealth.verdict as any)?.backup;
  const expected = layout(path.resolve(ADAPTER_ROOT, "..", "..", "..")).substrateBackups;
  add(checks, "substrate backup reported", Boolean(backup), backup ? "health.py reported its backup probe" : "health.py reported no backup block");
  if (backup) {
    add(
      checks,
      "substrate backup directory agreement",
      typeof backup.directory === "string" && comparablePath(backup.directory) === comparablePath(expected),
      `expected ${expected}; got ${String(backup.directory)}`,
    );
    add(
      checks,
      "substrate backup present",
      typeof backup.newest === "string" && backup.newest.length > 0,
      String(backup.error || backup.newest || "health.py found no dump in the backup directory"),
    );
  }
}

const staticFailed = checks.filter((check) => !check.ok && check.name !== "substrate runtime health");
const diagnostics: Diagnostic[] = [
  ...checks.filter((check) => !check.ok).map((check) => check.diagnostic || checkDiagnostic(check)),
  ...(Array.isArray(runtimeHealth.verdict?.diagnostics) ? runtimeHealth.verdict.diagnostics as Diagnostic[] : []),
];
const mode: VerificationMode = staticFailed.length !== 0
  ? "degraded"
  : profile === "vault"
    ? "Vault"
    : compatibleApis && runtimeHealth.ok === true
      ? "AKASHA"
      : "degraded";
const result = {
  ok: staticFailed.length === 0 && mode !== "degraded",
  staticOk: staticFailed.length === 0,
  mode,
  profile,
  adapterRoot: ADAPTER_ROOT,
  productRoot: ATHANOR_ROOT,
  stateRoot: String(process.env.ATHANOR_STATE_DIR || "").trim() || null,
  configPath,
  roomPath: roomArgument ? path.resolve(roomArgument) : null,
  substrateRoot,
  release: portableRelease,
  compatibilityPath: contractPath,
  compatibility: {
    ok: compatibleApis,
    expected: { substrateApi: 1, coreApi: 1, adapterApi: 1 },
    actual: compatibilityContract
      ? {
          substrateApi: compatibilityContract.substrateApi,
          coreApi: compatibilityContract.coreApi,
          adapterApi: compatibilityContract.adapterApi,
        }
      : null,
  },
  runtimeHealth,
  diagnostics,
  checks,
  next: mode === "AKASHA"
    ? "Start a fresh OMP session from the room directory and call room_state."
    : mode === "Vault"
      ? "Vault is statically verified; AKASHA memory is not configured."
      : "Fix substrate compatibility and runtime health, then rerun this verifier.",
};

console.log(JSON.stringify(result, null, 2));
if (!result.ok) process.exitCode = 1;

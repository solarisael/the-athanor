import { createHash } from "node:crypto";
import { lstatSync, readFileSync, realpathSync } from "node:fs";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

type Endpoint = { url: string; spirit: string };
type ClientProjection = {
  format: 1;
  houseId: string;
  hostToken: string;
  stateRoot: string;
  defaultRoom: string;
  endpoints: Record<string, Endpoint>;
};

type LoaderOptions = {
  programRoot?: string;
  userProfile?: string;
  env?: NodeJS.ProcessEnv;
};
type NativePointer = {
  version: string;
};


type Compatibility = {
  hostApi: number;
  substrateApi: number;
  deliveryApi: number;
  schemaVersion: number;
};

type ComponentPointer = {
  format: 1;
  releaseId: string;
  previousReleaseId: string | null;
};

type ComponentArtifact = {
  path: string;
  sha256: string;
  size: number;
};

type ComponentManifest = {
  format: 1;
  component: "omp-adapter";
  version: string;
  releaseId: string;
  compatibility: Compatibility;
  artifacts: ComponentArtifact[];
};

function requiredText(value: unknown, field: string): string {
  const text = String(value ?? "").trim();
  if (!text) throw new Error(`installed Athanor client projection has no ${field}`);
  return text;
}

function exactObject(
  value: unknown,
  label: string,
  requiredFields: readonly string[],
  optionalFields: readonly string[] = [],
): Record<string, unknown> {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new Error(`installed Athanor ${label} must be an object`);
  }
  const source = value as Record<string, unknown>;
  for (const field of requiredFields) {
    if (!Object.hasOwn(source, field)) {
      throw new Error(`installed Athanor ${label} has no ${field}`);
    }
  }
  const allowed = new Set([...requiredFields, ...optionalFields]);
  const unknown = Object.keys(source).find((field) => !allowed.has(field));
  if (unknown) {
    throw new Error(`installed Athanor ${label} has unknown field ${unknown}`);
  }
  return source;
}

type PhysicalDirectory = {
  logical: string;
  physical: string;
};

function isWithinPhysicalRoot(root: string, candidate: string): boolean {
  const relative = path.relative(root, candidate);
  return relative === ""
    || (!path.isAbsolute(relative) && relative !== ".." && !relative.startsWith(`..${path.sep}`));
}

function physicalDirectory(
  directory: string,
  label: string,
  parent?: PhysicalDirectory,
): PhysicalDirectory {
  const logical = path.resolve(directory);
  let status;
  try {
    status = lstatSync(logical);
  } catch (error) {
    throw new Error(`installed Athanor ${label} could not be inspected: ${String(error)}`);
  }
  if (status.isSymbolicLink() || !status.isDirectory()) {
    throw new Error(
      `installed Athanor ${label} must be a physical directory; symbolic links, junctions, and reparse points are refused`,
    );
  }
  let physical: string;
  try {
    physical = realpathSync(logical);
  } catch (error) {
    throw new Error(`installed Athanor ${label} physical path could not be resolved: ${String(error)}`);
  }
  if (parent && !isWithinPhysicalRoot(parent.physical, physical)) {
    throw new Error(`installed Athanor ${label} escapes its physical root`);
  }
  return { logical, physical };
}

function directoryWithin(
  root: PhysicalDirectory,
  parts: readonly string[],
  label: string,
  cache: Map<string, PhysicalDirectory> = new Map([[root.logical, root]]),
): PhysicalDirectory {
  let current = root;
  for (const part of parts) {
    const logical = path.join(current.logical, part);
    const cached = cache.get(logical);
    if (cached) {
      current = cached;
      continue;
    }
    current = physicalDirectory(logical, `${label} ancestor ${part}`, current);
    if (!isWithinPhysicalRoot(root.physical, current.physical)) {
      throw new Error(`installed Athanor ${label} ancestor ${part} escapes its physical root`);
    }
    cache.set(logical, current);
  }
  return current;
}

function regularFileWithin(
  root: PhysicalDirectory,
  relativePath: string,
  label: string,
  cache?: Map<string, PhysicalDirectory>,
): string {
  const parts = relativePath.split("/");
  const parent = directoryWithin(root, parts.slice(0, -1), label, cache);
  const file = path.join(parent.logical, parts.at(-1)!);
  let status;
  try {
    status = lstatSync(file);
  } catch (error) {
    throw new Error(`installed Athanor ${label} is missing or invalid: ${String(error)}`);
  }
  if (status.isSymbolicLink() || !status.isFile()) {
    throw new Error(
      `installed Athanor ${label} must be a regular physical file; symbolic links, junctions, and reparse points are refused`,
    );
  }
  let physical: string;
  try {
    physical = realpathSync(file);
  } catch (error) {
    throw new Error(`installed Athanor ${label} physical path could not be resolved: ${String(error)}`);
  }
  if (!isWithinPhysicalRoot(root.physical, physical)) {
    throw new Error(`installed Athanor ${label} escapes its physical root`);
  }
  return physical;
}

function readJson(file: string, label: string): unknown {
  let text: string;
  try {
    text = readFileSync(file, "utf8");
  } catch (error) {
    throw new Error(`installed Athanor ${label} could not be read: ${String(error)}`);
  }
  try {
    return JSON.parse(text);
  } catch (error) {
    throw new Error(`installed Athanor ${label} is not valid JSON: ${String(error)}`);
  }
}

function unsignedInteger(value: unknown, field: string): number {
  if (!Number.isSafeInteger(value) || Number(value) < 0) {
    throw new Error(`installed Athanor ${field} must be a nonnegative integer`);
  }
  return Number(value);
}

function unsigned32(value: unknown, field: string): number {
  const integer = unsignedInteger(value, field);
  if (integer > 0xffff_ffff) {
    throw new Error(`installed Athanor ${field} must be an unsigned 32-bit integer`);
  }
  return integer;
}

function safeVersion(value: unknown, field: string): string {
  if (typeof value !== "string"
    || value.length === 0
    || value.length > 128
    || !/^[0-9A-Za-z][0-9A-Za-z.+-]*$/.test(value)
    || value.includes("..")) {
    throw new Error(`installed Athanor ${field} is unsafe`);
  }
  return value;
}

function safeReleaseId(value: unknown, field: string): string {
  if (typeof value !== "string" || value.length < 66) {
    throw new Error(`installed Athanor ${field} is unsafe`);
  }
  const separator = value.length - 65;
  if (value[separator] !== "-" || !/^[0-9a-f]{64}$/.test(value.slice(separator + 1))) {
    throw new Error(`installed Athanor ${field} is unsafe`);
  }
  safeVersion(value.slice(0, separator), field);
  return value;
}

function asciiLower(value: string): string {
  return value.replace(/[A-Z]/g, (character) => character.toLowerCase());
}

function parseNativePointer(value: unknown): NativePointer {
  const source = exactObject(
    value,
    "current release pointer",
    ["version"],
    ["previousVersion", "rollbackBackup"],
  );
  const version = safeVersion(source.version, "current release version");
  if (source.previousVersion !== undefined && source.previousVersion !== null) {
    const previousVersion = safeVersion(source.previousVersion, "current release previousVersion");
    if (previousVersion === version) {
      throw new Error("installed Athanor current release pointer cannot name its active version as previous");
    }
  }
  if (source.rollbackBackup !== undefined
    && source.rollbackBackup !== null
    && typeof source.rollbackBackup !== "string") {
    throw new Error("installed Athanor current release rollbackBackup must be a string or null");
  }
  return { version };
}

function parsePointer(value: unknown): ComponentPointer {
  const source = exactObject(value, "OMP adapter pointer", ["format", "releaseId", "previousReleaseId"]);
  if (source.format !== 1) {
    throw new Error("installed Athanor OMP adapter pointer format is unsupported");
  }
  const releaseId = safeReleaseId(source.releaseId, "OMP adapter pointer releaseId");
  let previousReleaseId: string | null = null;
  if (source.previousReleaseId !== null) {
    previousReleaseId = safeReleaseId(source.previousReleaseId, "OMP adapter pointer previousReleaseId");
  }
  if (previousReleaseId === releaseId) {
    throw new Error("installed Athanor OMP adapter pointer cannot name its active release as previous");
  }
  return { format: 1, releaseId, previousReleaseId };
}

function parseCompatibility(value: unknown, label: string): Compatibility {
  const source = exactObject(value, label, ["hostApi", "substrateApi", "deliveryApi", "schemaVersion"]);
  return {
    hostApi: unsigned32(source.hostApi, `${label} hostApi`),
    substrateApi: unsigned32(source.substrateApi, `${label} substrateApi`),
    deliveryApi: unsigned32(source.deliveryApi, `${label} deliveryApi`),
    schemaVersion: unsigned32(source.schemaVersion, `${label} schemaVersion`),
  };
}

function safeArtifactPath(value: unknown, index: number): string {
  const label = `OMP adapter artifact ${index} path`;
  if (typeof value !== "string"
    || value.length === 0
    || value.startsWith("/")
    || value.includes("\\")
    || value.includes(":")
    || value.split("/").some((part) => !part || part === "." || part === "..")) {
    throw new Error(`installed Athanor ${label} is unsafe`);
  }
  return value;
}

function parseComponentManifest(value: unknown): ComponentManifest {
  const source = exactObject(value, "OMP adapter component manifest", [
    "format",
    "component",
    "version",
    "releaseId",
    "compatibility",
    "artifacts",
  ]);
  if (source.format !== 1) {
    throw new Error("installed Athanor OMP adapter component manifest format is unsupported");
  }
  if (source.component !== "omp-adapter") {
    throw new Error("installed Athanor OMP adapter component manifest names an unsupported component");
  }
  const version = safeVersion(source.version, "OMP adapter component version");
  const releaseId = safeReleaseId(source.releaseId, "OMP adapter component releaseId");
  const compatibility = parseCompatibility(
    source.compatibility,
    "OMP adapter component manifest compatibility",
  );
  if (!Array.isArray(source.artifacts)) {
    throw new Error("installed Athanor OMP adapter component manifest artifacts must be an array");
  }
  if (source.artifacts.length === 0) {
    throw new Error("installed Athanor OMP adapter component manifest has no artifacts");
  }
  const seen = new Set<string>();
  const artifacts = source.artifacts.map((value, index) => {
    const artifact = exactObject(value, `OMP adapter artifact ${index}`, ["path", "sha256", "size"]);
    const artifactPath = safeArtifactPath(artifact.path, index);
    const normalizedPath = asciiLower(artifactPath);
    if (seen.has(normalizedPath)) {
      throw new Error(`installed Athanor OMP adapter component manifest has duplicate artifact ${artifactPath}`);
    }
    seen.add(normalizedPath);
    if (typeof artifact.sha256 !== "string" || !/^[0-9a-f]{64}$/.test(artifact.sha256)) {
      throw new Error(`installed Athanor OMP adapter artifact ${artifactPath} has invalid SHA-256`);
    }
    return {
      path: artifactPath,
      sha256: artifact.sha256,
      size: unsignedInteger(artifact.size, `OMP adapter artifact ${artifactPath} size`),
    };
  });
  for (let index = 1; index < artifacts.length; index += 1) {
    if (artifacts[index - 1]!.path > artifacts[index]!.path) {
      throw new Error("installed Athanor OMP adapter component manifest artifacts are not ordinally sorted");
    }
  }
  return {
    format: 1,
    component: "omp-adapter",
    version,
    releaseId,
    compatibility,
    artifacts,
  };
}

function nativeCompatibility(value: unknown, version: string): Compatibility {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new Error("installed Athanor active native release manifest must be an object");
  }
  const source = value as Record<string, unknown>;
  if (source.format !== 1 || source.product !== "the-athanor") {
    throw new Error("installed Athanor active native release manifest identity is invalid");
  }
  const manifestVersion = safeVersion(source.version, "active native release manifest version");
  if (manifestVersion !== version) {
    throw new Error("installed Athanor active native release manifest identity is invalid");
  }
  if (source.platform !== "windows-x64") {
    throw new Error("installed Athanor active native release manifest platform is unsupported");
  }
  if (!source.compatibility || typeof source.compatibility !== "object" || Array.isArray(source.compatibility)) {
    throw new Error("installed Athanor active native release manifest has no compatibility object");
  }
  const compatibility = source.compatibility as Record<string, unknown>;
  return {
    hostApi: unsigned32(compatibility.hostApi, "active native release manifest compatibility hostApi"),
    substrateApi: unsigned32(compatibility.substrateApi, "active native release manifest compatibility substrateApi"),
    deliveryApi: unsigned32(compatibility.deliveryApi, "active native release manifest compatibility deliveryApi"),
    schemaVersion: unsigned32(source.schemaVersion, "active native release manifest schemaVersion"),
  };
}

function canonicalReleaseId(manifest: ComponentManifest): string {
  const lines = [
    "format=1",
    "component=omp-adapter",
    `version=${manifest.version}`,
    `hostApi=${manifest.compatibility.hostApi}`,
    `substrateApi=${manifest.compatibility.substrateApi}`,
    `deliveryApi=${manifest.compatibility.deliveryApi}`,
    `schemaVersion=${manifest.compatibility.schemaVersion}`,
    ...manifest.artifacts.map((artifact) => (
      `artifact=${artifact.path}\t${artifact.sha256}\t${artifact.size}`
    )),
  ];
  const identity = `${lines.join("\n")}\n`;
  return `${manifest.version}-${createHash("sha256").update(identity, "utf8").digest("hex")}`;
}

function verifyArtifacts(root: PhysicalDirectory, manifest: ComponentManifest) {
  const entrypoints = new Set(["index.ts", "hygiene.ts"]);
  const verifiedEntrypoints = new Map<string, string>();
  const directories = new Map<string, PhysicalDirectory>([[root.logical, root]]);
  for (const artifact of manifest.artifacts) {
    const file = regularFileWithin(
      root,
      artifact.path,
      `OMP adapter artifact ${artifact.path}`,
      directories,
    );
    const bytes = readFileSync(file);
    if (bytes.length !== artifact.size) {
      throw new Error(
        `installed Athanor OMP adapter artifact ${artifact.path} size mismatch: expected ${artifact.size}, got ${bytes.length}`,
      );
    }
    const digest = createHash("sha256").update(bytes).digest("hex");
    if (digest !== artifact.sha256) {
      throw new Error(`installed Athanor OMP adapter artifact ${artifact.path} SHA-256 mismatch`);
    }
    if (entrypoints.delete(artifact.path)) verifiedEntrypoints.set(artifact.path, file);
  }
  if (entrypoints.size > 0) {
    throw new Error(
      `installed Athanor OMP adapter component manifest omits entrypoint ${[...entrypoints].sort().join(", ")}`,
    );
  }
  return {
    index: verifiedEntrypoints.get("index.ts")!,
    hygiene: verifiedEntrypoints.get("hygiene.ts")!,
  };
}

function parseClientProjection(value: unknown): ClientProjection {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new Error("installed Athanor client projection must be an object");
  }
  const source = value as Record<string, unknown>;
  if (source.format !== 1) throw new Error("installed Athanor client projection format is unsupported");
  const rawEndpoints = source.endpoints;
  if (!rawEndpoints || typeof rawEndpoints !== "object" || Array.isArray(rawEndpoints)) {
    throw new Error("installed Athanor client projection has no endpoints map");
  }
  const endpoints: Record<string, Endpoint> = {};
  for (const [room, raw] of Object.entries(rawEndpoints as Record<string, unknown>)) {
    if (!raw || typeof raw !== "object" || Array.isArray(raw)) {
      throw new Error(`installed Athanor endpoint for room ${room} is invalid`);
    }
    const endpoint = raw as Record<string, unknown>;
    const url = requiredText(endpoint.url, `endpoint URL for room ${room}`);
    const parsed = new URL(url);
    if (parsed.protocol !== "ws:" || !["127.0.0.1", "localhost", "[::1]"].includes(parsed.hostname)) {
      throw new Error(`installed Athanor endpoint for room ${room} must be a loopback WebSocket`);
    }
    endpoints[room] = {
      url,
      spirit: requiredText(endpoint.spirit, `endpoint spirit for room ${room}`),
    };
  }
  const defaultRoom = requiredText(source.defaultRoom, "defaultRoom");
  if (!endpoints[defaultRoom]) {
    throw new Error("installed Athanor defaultRoom has no endpoint");
  }
  return {
    format: 1,
    houseId: requiredText(source.houseId, "houseId"),
    hostToken: requiredText(source.hostToken, "hostToken"),
    stateRoot: path.resolve(requiredText(source.stateRoot, "stateRoot")),
    defaultRoom,
    endpoints,
  };
}

function setUnlessConfigured(env: NodeJS.ProcessEnv, key: string, value: string) {
  if (!String(env[key] ?? "").trim()) env[key] = value;
}

export function configureInstalledAthanor(options: LoaderOptions = {}) {
  const env = options.env ?? process.env;
  const loaderDir = path.dirname(fileURLToPath(import.meta.url));
  const programRoot = physicalDirectory(
    options.programRoot ?? path.join(loaderDir, ".."),
    "program root",
  );
  const userProfilePath = path.resolve(options.userProfile ?? requiredText(env.USERPROFILE, "USERPROFILE"));
  const current = parseNativePointer(readJson(
    regularFileWithin(programRoot, "current.json", "current release pointer"),
    "current release pointer",
  ));
  const version = current.version;
  const nativeRoot = directoryWithin(
    programRoot,
    ["versions", version],
    "active native release",
  );
  const native = nativeCompatibility(
    readJson(
      regularFileWithin(nativeRoot, "release-manifest.json", "active native release manifest"),
      "active native release manifest",
    ),
    version,
  );

  const componentRoot = directoryWithin(
    programRoot,
    ["components", "omp-adapter"],
    "OMP adapter component root",
  );
  const pointer = parsePointer(readJson(
    regularFileWithin(componentRoot, "current.json", "OMP adapter pointer"),
    "OMP adapter pointer",
  ));
  const releaseRoot = directoryWithin(
    componentRoot,
    ["versions", pointer.releaseId],
    "OMP adapter release",
  );
  const manifest = parseComponentManifest(readJson(
    regularFileWithin(releaseRoot, "component-manifest.json", "OMP adapter component manifest"),
    "OMP adapter component manifest",
  ));
  if (manifest.releaseId !== pointer.releaseId) {
    throw new Error("installed Athanor OMP adapter component manifest releaseId does not match its pointer");
  }
  for (const field of ["hostApi", "substrateApi", "deliveryApi", "schemaVersion"] as const) {
    if (manifest.compatibility[field] !== native[field]) {
      throw new Error(
        `installed Athanor OMP adapter ${field} mismatch: component ${manifest.compatibility[field]}, native ${native[field]}`,
      );
    }
  }
  const expectedReleaseId = canonicalReleaseId(manifest);
  if (manifest.releaseId !== expectedReleaseId) {
    throw new Error(
      `installed Athanor OMP adapter releaseId mismatch: expected ${expectedReleaseId}, got ${manifest.releaseId}`,
    );
  }
  const entrypoints = verifyArtifacts(releaseRoot, manifest);

  const userProfile = physicalDirectory(userProfilePath, "operator profile root");
  const client = parseClientProjection(readJson(
    regularFileWithin(
      userProfile,
      ".omp/agent/athanor/client.json",
      "client projection",
    ),
    "client projection",
  ));

  setUnlessConfigured(env, "ATHANOR_STATE_DIR", client.stateRoot);
  setUnlessConfigured(env, "ATHANOR_SUBSTRATE_ROOT", nativeRoot.logical);
  setUnlessConfigured(env, "ATHANOR_SUBSTRATE_EXE", path.join(nativeRoot.logical, "bin", "athanor-substrate.exe"));
  setUnlessConfigured(env, "ATHANOR_HOST_HOUSE_ID", client.houseId);
  setUnlessConfigured(env, "ATHANOR_HOST_TOKEN", client.hostToken);
  setUnlessConfigured(env, "ATHANOR_HOST_ENDPOINTS", JSON.stringify(client.endpoints));

  return {
    index: pathToFileURL(entrypoints.index).href,
    hygiene: pathToFileURL(entrypoints.hygiene).href,
    releaseId: pointer.releaseId,
    previousReleaseId: pointer.previousReleaseId,
  };
}

export default async function installedAthanor(pi: unknown, options: LoaderOptions = {}) {
  const modules = configureInstalledAthanor(options);
  const [athanor, hygiene] = await Promise.all([import(modules.index), import(modules.hygiene)]);
  if (typeof athanor.default !== "function" || typeof hygiene.default !== "function") {
    throw new Error("installed Athanor extensions do not export OMP entrypoints");
  }
  // Pass-through, not new state: the entry receives the release this loader
  // resolved so the adapter never re-reads the pointer to name itself.
  await athanor.default(pi, {
    releaseId: modules.releaseId,
    previousReleaseId: modules.previousReleaseId,
  });
  await hygiene.default(pi);
}

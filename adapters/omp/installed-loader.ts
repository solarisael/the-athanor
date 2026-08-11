import { readFileSync } from "node:fs";
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

function requiredText(value: unknown, field: string): string {
  const text = String(value ?? "").trim();
  if (!text) throw new Error(`installed Athanor client projection has no ${field}`);
  return text;
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
  const programRoot = path.resolve(options.programRoot ?? path.join(loaderDir, ".."));
  const userProfile = path.resolve(options.userProfile ?? requiredText(env.USERPROFILE, "USERPROFILE"));
  const current = JSON.parse(readFileSync(path.join(programRoot, "current.json"), "utf8")) as Record<string, unknown>;
  const version = requiredText(current.version, "current release version");
  if (!/^[0-9A-Za-z][0-9A-Za-z.-]*$/.test(version)) {
    throw new Error("installed Athanor current release version is unsafe");
  }
  const versionRoot = path.join(programRoot, "versions", version);
  const clientPath = path.join(userProfile, ".omp", "agent", "athanor", "client.json");
  const client = parseClientProjection(JSON.parse(readFileSync(clientPath, "utf8")));

  setUnlessConfigured(env, "ATHANOR_STATE_DIR", client.stateRoot);
  setUnlessConfigured(env, "ATHANOR_SUBSTRATE_ROOT", versionRoot);
  setUnlessConfigured(env, "ATHANOR_SUBSTRATE_EXE", path.join(versionRoot, "bin", "athanor-substrate.exe"));
  setUnlessConfigured(env, "ATHANOR_HOST_HOUSE_ID", client.houseId);
  setUnlessConfigured(env, "ATHANOR_HOST_TOKEN", client.hostToken);
  setUnlessConfigured(env, "ATHANOR_HOST_ENDPOINTS", JSON.stringify(client.endpoints));

  return {
    index: pathToFileURL(path.join(versionRoot, "adapters", "omp", "index.ts")).href,
    hygiene: pathToFileURL(path.join(versionRoot, "adapters", "omp", "hygiene.ts")).href,
  };
}

export default async function installedAthanor(pi: unknown) {
  const modules = configureInstalledAthanor();
  const [athanor, hygiene] = await Promise.all([import(modules.index), import(modules.hygiene)]);
  if (typeof athanor.default !== "function" || typeof hygiene.default !== "function") {
    throw new Error("installed Athanor extensions do not export OMP entrypoints");
  }
  await athanor.default(pi);
  await hygiene.default(pi);
}

import { accessSync, constants as fsConstants, statSync } from "node:fs";
import path from "node:path";
// Importing ADAPTER_ROOT is also what guarantees ordering: athanor-root loads
// <install-root>/state/athanor.env as a module side effect, so an installed
// ATHANOR_SUBSTRATE_EXE / ATHANOR_AUTO is in place before anything below reads
// the environment.
import { ADAPTER_ROOT } from "./athanor-root.ts";

export type RustPlatform = "windows-x64" | "linux-x64" | "linux-arm64";
export type RustDiscoveryOptions = {
  env?: NodeJS.ProcessEnv;
  platform?: NodeJS.Platform;
  arch?: string;
  moduleDir?: string;
};

export function rustPlatform(platform: NodeJS.Platform = process.platform, arch = process.arch): RustPlatform | null {
  if (platform === "win32" && arch === "x64") return "windows-x64";
  if (platform === "linux" && arch === "x64") return "linux-x64";
  if (platform === "linux" && arch === "arm64") return "linux-arm64";
  return null;
}

export function rustBinaryName(platform: RustPlatform): string {
  return platform === "windows-x64" ? "athanor-substrate.exe" : "athanor-substrate";
}

function regularExecutable(candidate: string, explicit: boolean): string | null {
  try {
    const stat = statSync(candidate);
    if (!stat.isFile()) throw new Error("path is not a regular file");
    if (process.platform !== "win32") accessSync(candidate, fsConstants.X_OK);
    return candidate;
  } catch (error) {
    if (!explicit) return null;
    const reason = error instanceof Error ? error.message : String(error);
    throw new Error(`ATHANOR_SUBSTRATE_EXE points to an invalid executable: ${candidate} (${reason}). Set it to a regular executable file.`);
  }
}

function pathCandidates(name: string, env: NodeJS.ProcessEnv): string[] {
  const dirs = String(env.PATH || "").split(path.delimiter).filter(Boolean);
  return dirs.flatMap((dir) => {
    const candidate = path.join(dir, name);
    return process.platform === "win32" && !path.extname(name)
      ? [candidate, `${candidate}.exe`]
      : [candidate];
  });
}

/**
 * Resolve the substrate executable.
 *
 * `ATHANOR_SUBSTRATE_EXE` names it outright. Otherwise `ATHANOR_AUTO=1` opts
 * into portable discovery: the bundled `bin/<platform>/athanor-substrate`
 * first, then PATH. Without either, the substrate is simply not selected.
 */
export function discoverRustExecutable(options: RustDiscoveryOptions = {}): string | null {
  const env = options.env ?? process.env;
  const explicit = String(env.ATHANOR_SUBSTRATE_EXE || "").trim();
  if (explicit) return regularExecutable(path.resolve(explicit), true);
  if (String(env.ATHANOR_AUTO || "") !== "1") return null;
  const platform = rustPlatform(options.platform ?? process.platform, options.arch ?? process.arch);
  if (!platform) return null;
  const name = rustBinaryName(platform);
  // The bundled executable lives under the ADAPTER directory, not the product
  // root: <athanor>/adapters/omp/bin/<platform>/athanor-substrate[.exe]. That
  // is where the portable builder stages it and where verify-install hashes it.
  const moduleDir = options.moduleDir ?? ADAPTER_ROOT;
  const bundle = path.join(moduleDir, "bin", platform, name);
  const bundled = regularExecutable(bundle, false);
  if (bundled) return bundled;
  for (const candidate of pathCandidates(name, env)) {
    const found = regularExecutable(candidate, false);
    if (found) return found;
  }
  return null;
}

export function currentRustPlatform(): RustPlatform | null {
  return rustPlatform(process.platform, process.arch);
}

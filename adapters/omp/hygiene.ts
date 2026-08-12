// OMP-local filesystem hygiene: one pre-exec guard.
//
// A scratch-shaped write into a tracked tree is blocked before the tool runs,
// locally and synchronously, because the guard must hold even when nothing else
// of the House is up. Nothing else lives here: advisory house counsel, project
// state, lessons, and ranking are behavioral decisions owned by Rust.
import { existsSync } from "node:fs";
import os from "node:os";
import path from "node:path";

export function isScratchName(targetPath: string): boolean {
  const base = (targetPath.replace(/[\\/]+$/, "").split(/[\\/]/).pop() ?? "").toLowerCase();
  if (base.startsWith(".tmp_") || base.startsWith(".tmp.")) return true;
  if (base.endsWith(".tmp")) return true;
  if (base.includes("_scratch") || base.includes(".scratch")) return true;
  return /^_.*\.(ps1|sh|mjs|cjs)$/.test(base);
}

const TRACKED_MARKERS = [".git", ".omp", ".opencode"];
const VAULT_ROOT = process.env.SOLARISAEL_VAULT_ROOT || path.join(os.homedir(), "Solarisael");

export function dirHasTrackedMarker(directory: string): boolean {
  return TRACKED_MARKERS.some((marker) => existsSync(path.join(directory, marker)));
}

export function isInTrackedTree(
  absolutePath: string,
  hasMarker: (directory: string) => boolean = dirHasTrackedMarker,
): boolean {
  const resolved = path.resolve(absolutePath);
  const vault = path.resolve(VAULT_ROOT).toLowerCase();
  if (resolved.toLowerCase().startsWith(vault + path.sep)) return true;

  // ~/.omp and ~/.opencode are global configuration, not project markers.
  const home = path.resolve(os.homedir());
  let directory = path.dirname(resolved);
  let previous = "";
  while (directory && directory !== previous && directory !== home) {
    if (hasMarker(directory)) return true;
    previous = directory;
    directory = path.dirname(directory);
  }
  return false;
}

export function evaluateWrite(
  targetPath: string,
  hasMarker?: (directory: string) => boolean,
): { block: true; reason: string } | null {
  if (!targetPath) return null;
  const sanctionedScratch = /(^|[\\/])\.scratch([\\/]|$)/i.test(targetPath);
  if (sanctionedScratch || !isScratchName(targetPath) || !isInTrackedTree(targetPath, hasMarker)) {
    return null;
  }

  return {
    block: true,
    reason:
      `Refusing scratch write into a tracked tree: ${targetPath}\n` +
      "Throwaway files (.tmp_*, _*.ps1/.sh, *_scratch) never go in a synced/git tree — " +
      "they ride a blind 'git add' off-machine. Use a sandbox dir or the eval kernel " +
      "(no file at all). If this is a real deliverable, give it a real name and home.",
  };
}

export default function solarisaelHygiene(pi) {
  pi.setLabel?.("Solarisael Hygiene");

  pi.on("tool_call", async (event) => {
    if (event?.toolName !== "write") return;
    return evaluateWrite(String(event.input?.path ?? ""));
  });
}

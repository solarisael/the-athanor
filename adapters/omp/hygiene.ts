// OMP-local pre-exec guards.
//
// Scratch writes and malformed AST Edit metavariables block synchronously.
// Both guards must work when the rest of the House is unavailable.
// Advisory counsel, project state, lessons, and ranking remain Rust-owned.
import { existsSync } from "node:fs";
import os from "node:os";
import path from "node:path";

type ToolCallEvent = { toolName?: string; input?: Record<string, unknown> };
type Refusal = { block: true; reason: string };

const INVALID_AST_METAVARIABLE = /(?<!\$)\$\$(?!\$)[A-Za-z_][A-Za-z0-9_]*/;

function astEditParams(event: ToolCallEvent): Record<string, unknown> | null {
  if (event.toolName === "ast_edit") return event.input ?? {};
  if (event.toolName !== "write" || event.input?.path !== "xd://ast_edit") return null;
  if (typeof event.input.content !== "string") return null;
  try {
    const decoded = JSON.parse(event.input.content);
    return decoded && typeof decoded === "object" && !Array.isArray(decoded) ? decoded : null;
  } catch {
    // The mounted AST Edit device owns malformed JSON refusal.
    return null;
  }
}

export function evaluateAstEdit(event: ToolCallEvent): Refusal | null {
  const params = astEditParams(event);
  if (!params || !Array.isArray(params.ops)) return null;
  const invalid = params.ops.findIndex((op) => {
    if (!op || typeof op !== "object") return false;
    const candidate = op as Record<string, unknown>;
    return [candidate.pat, candidate.out].some((value) =>
      typeof value === "string" && INVALID_AST_METAVARIABLE.test(value)
    );
  });
  if (invalid < 0) return null;
  return {
    block: true,
    reason: `Refusing AST Edit operation ${invalid + 1}: $$NAME is invalid. Use $$$NAME for zero-or-more nodes.`,
  };
}

export function isScratchName(targetPath: string): boolean {
  const base = (targetPath.replace(/[\\/]+$/, "").split(/[\\/]/).pop() ?? "").toLowerCase();
  if (base.startsWith(".tmp_") || base.startsWith(".tmp.")) return true;
  if (base.endsWith(".tmp")) return true;
  if (base.includes("_scratch") || base.includes(".scratch")) return true;
  return /^_.*\.(ps1|sh|mjs|cjs)$/.test(base);
}

const TRACKED_MARKERS = [".git", ".omp", ".opencode"];
const VAULT_ROOT = process.env.ATHANOR_VAULT_ROOT || path.join(os.homedir(), "Solarisael");

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
): Refusal | null {
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
    const astEditRefusal = evaluateAstEdit(event);
    if (astEditRefusal) return astEditRefusal;
    if (event?.toolName !== "write") return;
    return evaluateWrite(String(event.input?.path ?? ""));
  });
}

// solarisael-hygiene — message-safe tool guardrails (OMP extension).
//
// Two surfaces, both chosen so they NEVER touch the message stream:
//   - tool_call (pre-exec): hard-block a scratch-shaped write into a tracked
//     tree. Returns { block, reason }; reason comes back as the tool error.
//   - tool_result (post-exec): soft-nudge after a bulk `git add` / forceful
//     `rm`. Prepends a <system-reminder> to the tool output, never aborts.
//
// Pure decision functions are exported for unit testing; the default factory
// only wires them to pi events. No TTSR, no abort, no replaceMessages.

import { existsSync } from "node:fs";
import path from "node:path";
import os from "node:os";

// A "scratch-shaped" name = throwaway artifact that must never land in a
// synced/tracked tree. Conservative on purpose: false positives erode trust.
export function isScratchName(p: string): boolean {
  const base = (p.replace(/[\\/]+$/, "").split(/[\\/]/).pop() ?? "").toLowerCase();
  if (base.startsWith(".tmp_") || base.startsWith(".tmp.")) return true;
  if (base.endsWith(".tmp")) return true;
  if (base.includes("_scratch") || base.includes(".scratch")) return true;
  if (/^_.*\.(ps1|sh|mjs|cjs)$/.test(base)) return true;
  return false;
}

// Sol's safety ask: a tree counts as "tracked" if it carries .git, .omp, OR
// .opencode (our strata grew up on opencode) — plus the Obsidian vault root.
// markerExists is injectable so the logic stays unit-testable without disk.
const TRACKED_MARKERS = [".git", ".omp", ".opencode"];
const VAULT_ROOT = "C:\\Solarisael\\Obsidian\\obsidian";

export function dirHasTrackedMarker(dir: string): boolean {
  for (const marker of TRACKED_MARKERS) {
    if (existsSync(path.join(dir, marker))) return true;
  }
  return false;
}

export function isInTrackedTree(
  absPath: string,
  hasMarker: (dir: string) => boolean = dirHasTrackedMarker,
): boolean {
  const resolved = path.resolve(absPath);
  const vault = path.resolve(VAULT_ROOT).toLowerCase();
  if (resolved.toLowerCase().startsWith(vault + path.sep)) return true;
  // Stop before the home dir: ~/.omp and ~/.opencode are GLOBAL config, not a
  // project marker — they must not make all of $HOME a "tracked tree".
  const home = path.resolve(os.homedir());
  let dir = path.dirname(resolved);
  let prev = "";
  while (dir && dir !== prev && dir !== home) {
    if (hasMarker(dir)) return true;
    prev = dir;
    dir = path.dirname(dir);
  }
  return false;
}

export function evaluateWrite(
  targetPath: string,
  hasMarker?: (dir: string) => boolean,
): { block: true; reason: string } | null {
  if (!targetPath) return null;
  // `.scratch/` is the sanctioned (gitignored) scratch home — never block there.
  const inScratchDir = /(^|[\\/])\.scratch([\\/]|$)/i.test(targetPath);
  if (!inScratchDir && isScratchName(targetPath) && isInTrackedTree(targetPath, hasMarker)) {
    return {
      block: true,
      reason:
        `Refusing scratch write into a tracked tree: ${targetPath}\n` +
        `Throwaway files (.tmp_*, _*.ps1/.sh, *_scratch) never go in a synced/git tree — ` +
        `they ride a blind 'git add' off-machine. Use a sandbox dir or the eval kernel ` +
        `(no file at all). If this is a real deliverable, give it a real name and home.`,
    };
  }
  return null;
}

const BULK_GIT_ADD = /\bgit\s+add\s+(?:-A\b|--all\b|\.(?:\s|$))/i;
const FORCEFUL_RM = /\brm\s+-[a-z]*[rf]/i;

export function evaluateBashNudge(command: string): string | null {
  if (!command) return null;
  if (BULK_GIT_ADD.test(command)) {
    return "bulk `git add` — read the staged set before committing (git status --short); don't sweep scratch, logs, or secrets off-machine.";
  }
  if (FORCEFUL_RM.test(command)) {
    return "forceful `rm` — verify what's actually gone afterward (test -f); deletions across the WSL/Windows boundary can fail silently.";
  }
  return null;
}

export default function solarisaelHygiene(pi) {
  pi.setLabel?.("Solarisael Hygiene");

  pi.on("tool_call", async (event) => {
    if (event.toolName !== "write") return;
    const decision = evaluateWrite(String(event.input?.path ?? ""));
    if (decision) return { block: true, reason: decision.reason };
  });

  pi.on("tool_result", async (event) => {
    if (event.isError || event.toolName !== "bash") return;
    const note = evaluateBashNudge(String(event.input?.command ?? ""));
    if (!note) return;
    const reminder = { type: "text", text: `<system-reminder>hygiene: ${note}</system-reminder>` };
    return { content: [reminder, ...(event.content ?? [])] };
  });
}

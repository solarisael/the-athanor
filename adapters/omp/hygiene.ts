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
import { existsSync, readFileSync } from "node:fs";
import path from "node:path";
import os from "node:os";
import { resolveObservedProject, type ProjectContext, type ToolCallLike } from "./solarisael-house-proof/project-context.ts";
import { runLessonContext, type LessonContext } from "./solarisael-house-proof/lesson-context.ts";
import { roomContext } from "./solarisael-house-proof/room.ts";
import { deriveLessonWorkingState, rememberActiveLessonState } from "./solarisael-house-proof/lesson-working-state.ts";

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

// Safety rule: a tree counts as "tracked" if it carries .git, .omp, OR
// .opencode (the adapter supports all three markers) — plus the Obsidian vault root.
// markerExists is injectable so the logic stays unit-testable without disk.
const TRACKED_MARKERS = [".git", ".omp", ".opencode"];
const VAULT_ROOT = process.env.SOLARISAEL_VAULT_ROOT || path.join(os.homedir(), "Solarisael");

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

export type HygieneDependencies = {
  resolveProject?: (call: ToolCallLike) => Promise<ProjectContext | null>;
  runLessons?: (input: Parameters<typeof runLessonContext>[0]) => Promise<LessonContext>;
};

const EXPLORATION = new Set(["read", "grep", "glob", "lsp", "ast_grep", "ast-grep", "bash"]);
const MUTATING = /^(edit|write|ast_edit|ast-edit|lsp(?:_|-)?(?:apply|edit|write|mutat))/i;

function callWithObservedInput(event: any): ToolCallLike {
  const input = event?.input && typeof event.input === "object" ? { ...(event.input as Record<string, unknown>) } : {};
  if (event?.toolName === "bash" && !("path" in input)) {
    const command = String(input.command ?? "");
    const match = command.match(/(?:[A-Za-z]:[\\/]|\/)[^"'`\s;|]+/);
    if (match) input.path = match[0];
  }
  return { name: event?.toolName, input };
}

function termsFor(toolName: string): string[] {
  const name = toolName.replace(/[-_]+/g, " ").trim().toLowerCase();
  return [...new Set([name, ...name.split(/\s+/).filter((term) => term.length > 2)])].slice(0, 4);
}
const LANGUAGE_BY_EXTENSION: Record<string, string> = {
  rs: "rust", gd: "gdscript", sql: "sql", ts: "typescript", tsx: "typescript",
  js: "javascript", jsx: "javascript", py: "python",
};

export function eligibilityContext(project: ProjectContext, target: string): {
  languages: string[]; technologies: string[];
} {
  const extension = /\.([a-z0-9]+)$/i.exec(target)?.[1]?.toLowerCase() || "";
  const languages = LANGUAGE_BY_EXTENSION[extension] ? [LANGUAGE_BY_EXTENSION[extension]] : [];
  const technologies = new Set<string>();
  if (existsSync(path.join(project.root, "project.godot"))) technologies.add("godot");
  const cargoManifest = path.join(project.root, "Cargo.toml");
  if (existsSync(cargoManifest)) {
    const cargo = readFileSync(cargoManifest, "utf8");
    if (/\b(?:sqlx|tokio-postgres|postgres)\b[\s\S]{0,240}\bpostgres/i.test(cargo)
        || /\b(?:tokio-postgres|postgres)\s*=/i.test(cargo)) {
      technologies.add("postgresql");
    }
  }
  return { languages, technologies: [...technologies].sort() };
}

function lessonReminder(project: ProjectContext, lessons: LessonContext): { type: "text"; text: string } {
  const compact = (value: unknown[]) => JSON.stringify(value.slice(0, 4));
  return {
    type: "text",
    text: [
      "<system-reminder>",
      `Active project: ${project.project} (${project.root})`,
      `Coding lessons: ${compact(lessons.codingLessons)}`,
      `Project lessons: ${compact(lessons.projectLessons)}`,
      "Hidden tool context only; do not persist or render.",
      "</system-reminder>",
    ].join("\n"),
  };
}

export default function solarisaelHygiene(pi, dependencies: HygieneDependencies = {}) {
  pi.setLabel?.("Solarisael Hygiene");
  const resolveProject = dependencies.resolveProject ?? ((call) => resolveObservedProject(call));
  const runLessons = dependencies.runLessons ?? runLessonContext;
  const queried = new Set<string>();
  const lessonCache = new Map<string, LessonContext>();

  async function inspect(event: any, ctx: any, preflight = false) {
    const toolName = String(event?.toolName ?? "");
    const call = callWithObservedInput(event);
    const project = await resolveProject(call).catch(() => null);
    if (!project) return null;
    const target = String(call.input?.path ?? call.input?.file ?? call.input?.cwd ?? "");
    rememberActiveLessonState(deriveLessonWorkingState({
      room: roomContext(ctx?.cwd).room, project, toolName: call.name, target,
    }));
    const eligibility = eligibilityContext(project, target);
    const key = `${project.root}\0${project.project}\0${eligibility.languages.join(",")}\0${eligibility.technologies.join(",")}`;
    const fresh = !queried.has(key);
    if (fresh) {
      queried.add(key);
      const room = roomContext(ctx?.cwd);
      const lessons = await runLessons({
        effectiveRoomDir: room.effectiveRoomDir,
        room: room.room,
        projects: [project.project],
        terms: termsFor(toolName),
        languages: eligibility.languages,
        technologies: eligibility.technologies,
        limit: 4,
      }).then((value) => ({
        codingLessons: Array.isArray(value?.codingLessons) ? value.codingLessons : [],
        projectLessons: Array.isArray(value?.projectLessons) ? value.projectLessons : [],
        match: value?.match && typeof value.match === "object" ? value.match : {},
      })).catch(() => ({ codingLessons: [], projectLessons: [], match: {} }));
      lessonCache.set(key, lessons);
      return { project, key, lessons, fresh };
    }
    const lessons = lessonCache.get(key) ?? { codingLessons: [], projectLessons: [], match: {} };
    return { project, key, lessons, fresh };
  }

  pi.on("tool_call", async (event, ctx) => {
    const toolName = String(event?.toolName ?? "");
    if (toolName === "write") {
      const decision = evaluateWrite(String(event.input?.path ?? ""));
      if (decision) return { block: true, reason: decision.reason };
    }
    if (!MUTATING.test(toolName)) return;
    const observed = await inspect(event, ctx, true);
    if (!observed || !observed.fresh) return;
    const hasLessons = observed.lessons.codingLessons.length || observed.lessons.projectLessons.length;
    if (hasLessons) {
      return {
        block: true,
        reason: `Project preflight loaded for ${observed.project.project}. Retry this ${toolName} call so the automatic project context is applied.`,
      };
    }
  });

  pi.on("tool_result", async (event, ctx) => {
    if (event?.isError) return;
    const toolName = String(event?.toolName ?? "");
    const note = toolName === "bash" ? evaluateBashNudge(String(event.input?.command ?? "")) : null;
    const shouldInspect = EXPLORATION.has(toolName);
    const observed = shouldInspect ? await inspect(event, ctx) : null;
    const content = Array.isArray(event?.content) ? event.content : [];
    const additions = [];
    if (note) additions.push({ type: "text", text: `<system-reminder>hygiene: ${note}</system-reminder>` });
    if (observed?.fresh) additions.push(lessonReminder(observed.project, observed.lessons));
    if (!additions.length) return;
    return { content: [...additions, ...content] };
  });
}

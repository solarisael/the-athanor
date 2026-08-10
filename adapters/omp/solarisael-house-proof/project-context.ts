import { promises as fs } from "node:fs";
import path from "node:path";

export type ToolCallLike = {
  name?: string;
  arguments?: unknown;
  params?: unknown;
  input?: unknown;
};
export type ProjectAlias = { root: string; project: string; roots?: string[] };
export type ProjectContext = {
  project: string;
  root: string;
  source: "alias" | "marker" | "metadata" | "repository";
  candidates: string[];
};
export type ProjectResolverOptions = {
  aliases?: Record<string, string> | ProjectAlias[];
  projectAliases?: Record<string, string> | ProjectAlias[];
  markerNames?: string[];
  metadataNames?: string[];
};

const PATH_KEYS = new Set(["path", "paths", "file", "cwd"]);
const DEFAULT_MARKERS = ["package.json", "pyproject.toml", "Cargo.toml", "go.mod", "deno.json", "composer.json"];

function isAbsolute(value: string): boolean {
  return path.posix.isAbsolute(value) || path.win32.isAbsolute(value) || /^[A-Za-z]:[\\/]/.test(value) || /^\\\\/.test(value);
}

function cleanAbsolute(value: string): string | null {
  const text = value.trim();
  if (!text || !isAbsolute(text)) return null;
  return path.normalize(text.replace(/[\\/]+/g, path.sep));
}

function valuesForKey(value: unknown): unknown[] {
  if (typeof value === "string") return value.split(";");
  if (Array.isArray(value)) return value;
  return [];
}

/** Extracts only absolute paths; room/process cwd is deliberately not consulted. */
export function extractCandidatePaths(toolCall: ToolCallLike | unknown): string[] {
  const raw = toolCall && typeof toolCall === "object" ? toolCall as Record<string, unknown> : {};
  const args = (raw.arguments ?? raw.params ?? raw.input ?? raw) as unknown;
  if (!args || typeof args !== "object") return [];
  const found: string[] = [];
  const visit = (obj: Record<string, unknown>) => {
    for (const [key, value] of Object.entries(obj)) {
      if (!PATH_KEYS.has(key)) continue;
      for (const item of valuesForKey(value)) {
        if (typeof item !== "string") continue;
        const candidate = cleanAbsolute(item);
        if (candidate && !found.includes(candidate)) found.push(candidate);
      }
    }
  };
  visit(args as Record<string, unknown>);
  return found;
}

function aliasEntries(options: ProjectResolverOptions): ProjectAlias[] {
  const source = options.aliases ?? options.projectAliases ?? {};
  if (Array.isArray(source)) return source.map((a) => ({ root: path.normalize(a.root), project: a.project })).filter((a) => a.root && a.project);
  return Object.entries(source).map(([root, project]) => ({ root: path.normalize(root), project })).filter((a) => a.root && a.project);
}

function within(candidate: string, root: string): boolean {
  const relative = path.relative(root, candidate);
  return relative === "" || (relative !== ".." && !relative.startsWith(`..${path.sep}`) && !path.isAbsolute(relative));
}

async function exists(file: string): Promise<boolean> {
  try { await fs.access(file); return true; } catch { return false; }
}

async function readProjectMarker(root: string, markerNames: string[]): Promise<string | null> {
  for (const name of markerNames) {
    try {
      const parsed = JSON.parse(await fs.readFile(path.join(root, name), "utf8"));
      const project = parsed?.project ?? parsed?.name ?? parsed?.id;
      if (typeof project === "string" && project.trim()) return project.trim();
    } catch { /* fail open */ }
  }
  return null;
}

/** Resolve one observed absolute path, walking upward only for marker discovery. */
export async function resolveProjectRoot(candidate: string, options: ProjectResolverOptions = {}): Promise<ProjectContext | null> {
  const absolute = cleanAbsolute(candidate);
  if (!absolute) return null;
  const aliases = aliasEntries(options);
  const alias = aliases.filter((item) => within(absolute, item.root)).sort((a, b) => b.root.length - a.root.length)[0];
  if (alias) return { project: alias.project, root: alias.root, source: "alias", candidates: [absolute] };

  let current = absolute;
  try { if (!(await fs.stat(current)).isDirectory()) current = path.dirname(current); } catch { current = path.dirname(current); }
  const markerNames = options.markerNames ?? [".solarisael-project.json"];
  const metadataNames = options.metadataNames ?? DEFAULT_MARKERS;
  let repositoryRoot: string | null = null;
  while (true) {
    const marker = await readProjectMarker(current, markerNames);
    if (marker) return { project: marker, root: current, source: "marker", candidates: [absolute] };
    if (await exists(path.join(current, ".git"))) { repositoryRoot ??= current; }
    for (const metadataName of metadataNames) {
      if (metadataName !== "package.json" || !(await exists(path.join(current, metadataName)))) continue;
      try {
        const pkg = JSON.parse(await fs.readFile(path.join(current, metadataName), "utf8"));
        const project = pkg?.name ?? pkg?.project ?? pkg?.id;
        if (typeof project === "string" && project.trim()) return { project: project.trim(), root: current, source: "metadata", candidates: [absolute] };
      } catch { /* continue upward */ }
    }
    const parent = path.dirname(current);
    if (parent === current) break;
    current = parent;
  }
  const root = repositoryRoot ?? path.dirname(absolute);
  return { project: path.basename(root) || root, root, source: "repository", candidates: [absolute] };
}

export async function resolveObservedProject(toolCall: ToolCallLike | unknown, options: ProjectResolverOptions = {}): Promise<ProjectContext | null> {
  const raw = toolCall && typeof toolCall === "object" ? toolCall as Record<string, unknown> : {};
  const args = (raw.arguments ?? raw.params ?? raw.input ?? raw) as unknown;
  if (args && typeof args === "object") {
    const keys = Object.keys(args as Record<string, unknown>).filter((key) => PATH_KEYS.has(key));
    if (keys.length > 0 && keys.every((key) => key === "cwd")) return null;
  }
  const candidates = extractCandidatePaths(toolCall);
  for (const candidate of candidates) {
    const result = await resolveProjectRoot(candidate, options);
    if (result) return { ...result, candidates };
  }
  return null;
}

export function createProjectContextResolver(options: ProjectResolverOptions = {}) {
  const active = new Map<string, ProjectContext>();
  return {
    async observe(toolCall: ToolCallLike | unknown): Promise<ProjectContext | null> {
      const context = await resolveObservedProject(toolCall, options);
      if (context) active.set(context.root, context);
      return context;
    },
    activeProjects(): ProjectContext[] { return [...active.values()]; },
    clear(): void { active.clear(); },
  };
}

export const projectFromToolCall = resolveObservedProject;
export const candidatePathsFromToolCall = extractCandidatePaths;

import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { mkdir, rm, writeFile } from "node:fs/promises";
import path from "node:path";
import { createProjectContextResolver, extractCandidatePaths, resolveObservedProject } from "../solarisael-house-proof/project-context.ts";

const root = path.join(process.env.TEMP || process.env.TMP || ".", `solarisael-project-context-${Date.now()}`);
const repos = ["house", "house-omp", "house-proof"].map((name) => path.join(root, name));

beforeAll(async () => {
  await Promise.all(repos.map(async (repo) => {
    await mkdir(path.join(repo, ".git"), { recursive: true });
    await mkdir(path.join(repo, "src"), { recursive: true });
  }));
  await writeFile(path.join(repos[0], "package.json"), JSON.stringify({ name: "the-athanor" }));
});
afterAll(async () => { await rm(root, { recursive: true, force: true }); });

describe("project context", () => {
  test("does not turn room cwd into a project", async () => {
    expect(extractCandidatePaths({ name: "read", arguments: { path: "src/index.ts", cwd: root } })).toEqual([root]);
    expect(await resolveObservedProject({ name: "turn", arguments: { cwd: root } })).toBeNull();
  });

  test("resolves an absolute read path to its repository", async () => {
    const result = await resolveObservedProject({ name: "read", arguments: { path: path.join(repos[0], "src", "index.ts") } });
    expect(result?.root).toBe(repos[0]);
    expect(result?.project).toBe("the-athanor");
  });

  test("aliases the three House repository roots", async () => {
    const aliases = Object.fromEntries(repos.map((repo) => [repo, "the-athanor"]));
    for (const repo of repos) {
      const result = await resolveObservedProject({ arguments: { path: path.join(repo, "src") } }, { aliases });
      expect(result?.project).toBe("the-athanor");
      expect(result?.root).toBe(repo);
    }
  });

  test("switching paths changes active project while retaining multiple roots", async () => {
    const resolver = createProjectContextResolver();
    const first = await resolver.observe({ arguments: { file: path.join(repos[1], "src", "a.ts") } });
    const second = await resolver.observe({ arguments: { paths: `${path.join(repos[2], "src", "b.ts")};${path.join(repos[1], "src", "c.ts")}` } });
    expect(first?.root).toBe(repos[1]);
    expect(second?.root).toBe(repos[2]);
    expect(resolver.activeProjects().map((item) => item.root)).toEqual([repos[1], repos[2]]);
  });

  test("relative and unresolved paths fail safely", async () => {
    expect(await resolveObservedProject({ arguments: { file: "relative.ts" } })).toBeNull();
    const result = await resolveObservedProject({ arguments: { path: path.join(root, "missing", "file.ts") } });
    expect(result).not.toBeNull();
    expect(result?.root).toBe(path.join(root, "missing"));
  });
});

import { afterEach, expect, test } from "bun:test";
import { createHash, randomUUID } from "node:crypto";
import { mkdir, readFile, rename, rm, symlink, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import installedAthanor, { configureInstalledAthanor } from "../installed-loader.ts";

type NativePointer = {
  version: unknown;
  previousVersion?: unknown;
  rollbackBackup?: unknown;
  [field: string]: unknown;
};

type NativeManifest = {
  format: unknown;
  product: unknown;
  version: unknown;
  platform: unknown;
  schemaVersion: unknown;
  compatibility: Record<string, unknown>;
  [field: string]: unknown;
};

type Compatibility = {
  hostApi: number;
  substrateApi: number;
  deliveryApi: number;
  schemaVersion: number;
  [field: string]: unknown;
};

type Artifact = {
  path: string;
  sha256: string;
  size: number;
  [field: string]: unknown;
};

type Manifest = {
  format: number;
  component: string;
  version: string;
  releaseId: string;
  compatibility: Compatibility;
  artifacts: Artifact[];
  [field: string]: unknown;
};

type Pointer = {
  format: number;
  releaseId: string;
  previousReleaseId?: string | null;
  [field: string]: unknown;
};

type InstalledTree = {
  root: string;
  program: string;
  profile: string;
  nativeRoot: string;
  nativePointerPath: string;
  nativeManifestPath: string;
  releaseRoot: string;
  componentRoot: string;
  manifestPath: string;
  pointerPath: string;
  manifest: Manifest;
  pointer: Pointer;
  nativePointer: NativePointer;
  nativeManifest: NativeManifest;
  env: NodeJS.ProcessEnv;
};

type LoaderRuntime = typeof globalThis & {
  __installedLoaderImports?: string[];
  __installedLoaderCalls?: string[];
  __installedLoaderRelease?: unknown;
};

const roots: string[] = [];
const compatibility: Compatibility = {
  hostApi: 1,
  substrateApi: 1,
  deliveryApi: 1,
  schemaVersion: 18,
};

const adapterSources: Record<string, string> = {
  "hygiene.ts": [
    "const state = globalThis as typeof globalThis & { __installedLoaderImports?: string[]; __installedLoaderCalls?: string[] };",
    "(state.__installedLoaderImports ??= []).push('hygiene');",
    "export default async function hygiene() { (state.__installedLoaderCalls ??= []).push('hygiene'); }",
    "",
  ].join("\n"),
  "index.ts": [
    "const state = globalThis as typeof globalThis & { __installedLoaderImports?: string[]; __installedLoaderCalls?: string[]; __installedLoaderRelease?: unknown };",
    "(state.__installedLoaderImports ??= []).push('index');",
    "export default async function index(_pi, release) { (state.__installedLoaderCalls ??= []).push('index'); state.__installedLoaderRelease = release; }",
    "",
  ].join("\n"),
  "solarisael-house-proof/constants.ts": "export const INSTALLED_COMPONENT_PROOF = 1;\n",
};

afterEach(async () => {
  delete (globalThis as LoaderRuntime).__installedLoaderImports;
  delete (globalThis as LoaderRuntime).__installedLoaderCalls;
  delete (globalThis as LoaderRuntime).__installedLoaderRelease;
  await Promise.all(roots.splice(0).map((root) => rm(root, { recursive: true, force: true })));
});

function sha256(value: string | Uint8Array): string {
  return createHash("sha256").update(value).digest("hex");
}

function releaseIdFor(manifest: Omit<Manifest, "releaseId"> | Manifest): string {
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
  return `${manifest.version}-${sha256(`${lines.join("\n")}\n`)}`;
}

function ordinalCompare(left: string, right: string): number {
  if (left < right) return -1;
  if (left > right) return 1;
  return 0;
}

function runtime(): LoaderRuntime {
  return globalThis as LoaderRuntime;
}

function resetRuntime() {
  delete runtime().__installedLoaderImports;
  delete runtime().__installedLoaderCalls;
  delete runtime().__installedLoaderRelease;
}

async function persistNativePointer(tree: InstalledTree) {
  await writeFile(tree.nativePointerPath, JSON.stringify(tree.nativePointer));
}

async function persistNativeManifest(tree: InstalledTree) {
  await writeFile(tree.nativeManifestPath, JSON.stringify(tree.nativeManifest));
}

async function persistPointer(tree: InstalledTree) {
  await writeFile(tree.pointerPath, JSON.stringify(tree.pointer));
}

async function persistManifest(tree: InstalledTree) {
  await writeFile(tree.manifestPath, JSON.stringify(tree.manifest));
}
async function expectFreshProcessRefusal(tree: InstalledTree, message: string | RegExp) {
  const probe = path.join(tree.root, `loader-probe-${randomUUID()}.ts`);
  await writeFile(
    probe,
    [
      `import { configureInstalledAthanor } from ${JSON.stringify(new URL("../installed-loader.ts", import.meta.url).href)};`,
      "try {",
      `  configureInstalledAthanor(${JSON.stringify({
        programRoot: tree.program,
        userProfile: tree.profile,
        env: {},
      })});`,
      "  console.error('loader unexpectedly accepted the installed tree');",
      "  process.exitCode = 2;",
      "} catch (error) {",
      "  console.error(error instanceof Error ? error.message : String(error));",
      "}",
    ].join("\n"),
    "utf8",
  );
  const child = Bun.spawnSync({
    cmd: [process.execPath, "run", probe],
    cwd: os.tmpdir(),
    env: { PATH: process.env.PATH ?? "" },
    stdout: "pipe",
    stderr: "pipe",
  });
  const stderr = child.stderr.toString();
  expect(child.exitCode).toBe(0);
  if (typeof message === "string") expect(stderr).toContain(message);
  else expect(stderr).toMatch(message);
}


async function makeInstalledTree(
  previousReleaseId: string | null = null,
  nativeVersion = "0.10.1",
  componentVersion = "0.9.3",
): Promise<InstalledTree> {
  const root = path.join(os.tmpdir(), `athanor-loader-${randomUUID()}`);
  roots.push(root);
  const program = path.join(root, "program");
  const profile = path.join(root, "operator");
  const nativeRoot = path.join(program, "versions", nativeVersion);
  await mkdir(nativeRoot, { recursive: true });
  await mkdir(path.join(profile, ".omp", "agent", "athanor"), { recursive: true });
  const nativePointer = {
    version: nativeVersion,
    previousVersion: "0.10.0",
  } satisfies NativePointer;
  const nativePointerPath = path.join(program, "current.json");
  await writeFile(nativePointerPath, JSON.stringify(nativePointer));
  const nativeManifest = {
    format: 1,
    product: "the-athanor",
    version: nativeVersion,
    platform: "windows-x64",
    schemaVersion: 18,
    compatibility: {
      hostApi: 1,
      substrateApi: 1,
      deliveryApi: 1,
      godotApi: "4.7",
    },
  } satisfies NativeManifest;
  const nativeManifestPath = path.join(nativeRoot, "release-manifest.json");
  await writeFile(nativeManifestPath, JSON.stringify(nativeManifest));
  await writeFile(path.join(profile, ".omp", "agent", "athanor", "client.json"), JSON.stringify({
    format: 1,
    houseId: "solarisael",
    hostToken: "private-token",
    stateRoot: path.join(root, "state"),
    defaultRoom: "kintsu",
    endpoints: {
      kintsu: { url: "ws://127.0.0.1:8787/athanor/v1/ws", spirit: "Kintsu" },
      kodo: { url: "ws://127.0.0.1:8788/athanor/v1/ws", spirit: "Kodo" },
    },
  }));

  const artifacts = Object.entries(adapterSources)
    .map(([artifactPath, source]) => ({
      path: artifactPath,
      sha256: sha256(source),
      size: Buffer.byteLength(source),
    }))
    .sort((left, right) => ordinalCompare(left.path, right.path));
  const manifest = {
    format: 1,
    component: "omp-adapter",
    version: componentVersion,
    releaseId: "",
    compatibility: { ...compatibility },
    artifacts,
  } satisfies Manifest;
  manifest.releaseId = releaseIdFor(manifest);

  const componentRoot = path.join(program, "components", "omp-adapter");
  const releaseRoot = path.join(componentRoot, "versions", manifest.releaseId);
  await mkdir(releaseRoot, { recursive: true });
  for (const [artifactPath, source] of Object.entries(adapterSources)) {
    const file = path.join(releaseRoot, ...artifactPath.split("/"));
    await mkdir(path.dirname(file), { recursive: true });
    await writeFile(file, source);
  }
  const manifestPath = path.join(releaseRoot, "component-manifest.json");
  await writeFile(manifestPath, JSON.stringify(manifest));
  const pointer = {
    format: 1,
    releaseId: manifest.releaseId,
    previousReleaseId,
  } satisfies Pointer;
  const pointerPath = path.join(componentRoot, "current.json");
  await writeFile(pointerPath, JSON.stringify(pointer));
  return {
    root,
    program,
    profile,
    nativeRoot,
    nativePointerPath,
    nativeManifestPath,
    releaseRoot,
    componentRoot,
    manifestPath,
    pointerPath,
    manifest,
    pointer,
    nativePointer,
    nativeManifest,
    env: {},
  };
}

async function expectRefusal(tree: InstalledTree, message: string | RegExp) {
  resetRuntime();
  await expect(installedAthanor(null, {
    programRoot: tree.program,
    userProfile: tree.profile,
    env: tree.env,
  })).rejects.toThrow(message);
  expect(runtime().__installedLoaderImports ?? []).toEqual([]);
  expect(runtime().__installedLoaderCalls ?? []).toEqual([]);
}

test("loads the integrity-checked component release while native current.json owns runtime configuration", async () => {
  const previousReleaseId = `0.9.2-${"a".repeat(64)}`;
  const tree = await makeInstalledTree(previousReleaseId);
  tree.env.ATHANOR_STATE_DIR = path.join(tree.root, "operator-override");

  const modules = configureInstalledAthanor({
    programRoot: tree.program,
    userProfile: tree.profile,
    env: tree.env,
  });

  expect(modules.releaseId).toBe(tree.manifest.releaseId);
  expect(modules.previousReleaseId).toBe(previousReleaseId);
  expect(modules.index).toContain(`/components/omp-adapter/versions/${tree.manifest.releaseId}/index.ts`);
  expect(modules.hygiene).toContain(`/components/omp-adapter/versions/${tree.manifest.releaseId}/hygiene.ts`);
  expect(modules.index).not.toContain("/adapters/omp/");
  expect(modules.index).not.toContain("private-token");
  expect(tree.env.ATHANOR_STATE_DIR).toBe(path.join(tree.root, "operator-override"));
  expect(tree.env.ATHANOR_SUBSTRATE_ROOT).toBe(tree.nativeRoot);
  expect(tree.env.ATHANOR_SUBSTRATE_EXE).toBe(path.join(tree.nativeRoot, "bin", "athanor-substrate.exe"));
  expect(tree.env.ATHANOR_HOST_HOUSE_ID).toBe("solarisael");
  expect(tree.env.ATHANOR_HOST_TOKEN).toBe("private-token");
  expect(JSON.parse(tree.env.ATHANOR_HOST_ENDPOINTS!)).toEqual({
    kintsu: { url: "ws://127.0.0.1:8787/athanor/v1/ws", spirit: "Kintsu" },
    kodo: { url: "ws://127.0.0.1:8788/athanor/v1/ws", spirit: "Kodo" },
  });

  await installedAthanor(null, {
    programRoot: tree.program,
    userProfile: tree.profile,
    env: tree.env,
  });
  expect([...(runtime().__installedLoaderImports ?? [])].sort()).toEqual(["hygiene", "index"]);
  expect(runtime().__installedLoaderCalls).toEqual(["index", "hygiene"]);
});

test("accepts explicit null previous release metadata", async () => {
  const tree = await makeInstalledTree(null);
  const modules = configureInstalledAthanor({
    programRoot: tree.program,
    userProfile: tree.profile,
    env: tree.env,
  });
  expect(modules.previousReleaseId).toBeNull();
});

// The adapter reports "installed != loaded" from what the loader handed it, so
// the entry must receive the release the loader actually resolved. A loader that
// silently drops it leaves the adapter naming nothing.
test("hands the resolved release through to the adapter entry", async () => {
  const previousReleaseId = `0.9.2-${"c".repeat(64)}`;
  const tree = await makeInstalledTree(previousReleaseId);
  resetRuntime();

  await installedAthanor(null, {
    programRoot: tree.program,
    userProfile: tree.profile,
    env: tree.env,
  });

  expect(runtime().__installedLoaderRelease).toEqual({
    releaseId: tree.manifest.releaseId,
    previousReleaseId,
  });
});

test("hands through an explicit null previous release rather than omitting it", async () => {
  const tree = await makeInstalledTree(null);
  resetRuntime();

  await installedAthanor(null, {
    programRoot: tree.program,
    userProfile: tree.profile,
    env: tree.env,
  });

  expect(runtime().__installedLoaderRelease).toEqual({
    releaseId: tree.manifest.releaseId,
    previousReleaseId: null,
  });
});

test("parses pointer and component manifest objects exactly", async () => {
  const cases: Array<{
    mutate: (tree: InstalledTree) => void;
    persist: (tree: InstalledTree) => Promise<void>;
    message: string;
  }> = [
    {
      mutate: (tree) => { tree.pointer.format = 2; },
      persist: persistPointer,
      message: "OMP adapter pointer format is unsupported",
    },
    {
      mutate: (tree) => { tree.manifest.format = 2; },
      persist: persistManifest,
      message: "component manifest format is unsupported",
    },
    {
      mutate: (tree) => { tree.manifest.component = "other"; },
      persist: persistManifest,
      message: "component manifest names an unsupported component",
    },
    {
      mutate: (tree) => { tree.pointer.unexpected = true; },
      persist: persistPointer,
      message: "OMP adapter pointer has unknown field unexpected",
    },
    {
      mutate: (tree) => { delete tree.pointer.previousReleaseId; },
      persist: persistPointer,
      message: "OMP adapter pointer has no previousReleaseId",
    },
    {
      mutate: (tree) => { tree.manifest.unexpected = true; },
      persist: persistManifest,
      message: "component manifest has unknown field unexpected",
    },
    {
      mutate: (tree) => { tree.manifest.compatibility.unexpected = true; },
      persist: persistManifest,
      message: "manifest compatibility has unknown field unexpected",
    },
    {
      mutate: (tree) => { tree.manifest.artifacts[0]!.unexpected = true; },
      persist: persistManifest,
      message: "artifact 0 has unknown field unexpected",
    },
    {
      mutate: (tree) => { tree.manifest.artifacts[0]!.sha256 = "A".repeat(64); },
      persist: persistManifest,
      message: "has invalid SHA-256",
    },
    {
      mutate: (tree) => { tree.manifest.artifacts[0]!.size = -1; },
      persist: persistManifest,
      message: "size must be a nonnegative integer",
    },
  ];
  for (const { mutate, persist, message } of cases) {
    const tree = await makeInstalledTree();
    mutate(tree);
    await persist(tree);
    await expectRefusal(tree, message);
  }
});

test("refuses unsafe current and previous component release IDs before import", async () => {
  for (const [field, value] of [
    ["releaseId", "../escape"],
    ["releaseId", `0.9.3-${"A".repeat(64)}`],
    ["previousReleaseId", "../escape"],
  ] as const) {
    const tree = await makeInstalledTree();
    tree.pointer[field] = value;
    await persistPointer(tree);
    await expectRefusal(tree, `pointer ${field} is unsafe`);
  }

  const cycle = await makeInstalledTree();
  cycle.pointer.previousReleaseId = cycle.pointer.releaseId;
  await persistPointer(cycle);
  await expectRefusal(cycle, "cannot name its active release as previous");

  for (const version of ["../0.9.3", "0.9_3", "v".repeat(129)]) {
    const tree = await makeInstalledTree();
    tree.manifest.version = version;
    await persistManifest(tree);
    await expectRefusal(tree, "component version is unsafe");
  }
});

test("refuses unsafe, duplicate, and unsorted artifact paths before import", async () => {
  for (const unsafePath of [
    "../index.ts",
    "/index.ts",
    "C:/index.ts",
    "nested\\index.ts",
    "./index.ts",
    "nested//index.ts",
    "nested/file:stream.ts",
  ]) {
    const tree = await makeInstalledTree();
    tree.manifest.artifacts[0]!.path = unsafePath;
    await persistManifest(tree);
    await expectRefusal(tree, "path is unsafe");
  }

  const duplicate = await makeInstalledTree();
  duplicate.manifest.artifacts[1]!.path = duplicate.manifest.artifacts[0]!.path;
  await persistManifest(duplicate);
  await expectRefusal(duplicate, "duplicate artifact");

  const caseDuplicate = await makeInstalledTree();
  caseDuplicate.manifest.artifacts[0]!.path = "HYGIENE.TS";
  caseDuplicate.manifest.artifacts[1]!.path = "hygiene.ts";
  await persistManifest(caseDuplicate);
  await expectRefusal(caseDuplicate, "duplicate artifact");

  const unsorted = await makeInstalledTree();
  unsorted.manifest.artifacts.reverse();
  await persistManifest(unsorted);
  await expectRefusal(unsorted, "artifacts are not ordinally sorted");
});

test("refuses a missing declared artifact before import", async () => {
  const tree = await makeInstalledTree();
  await rm(path.join(tree.componentRoot, "versions", tree.manifest.releaseId, "hygiene.ts"));
  await expectRefusal(tree, /artifact hygiene\.ts is missing or invalid/);
});

test("refuses artifact size tampering before import", async () => {
  const tree = await makeInstalledTree();
  const file = path.join(
    tree.componentRoot,
    "versions",
    tree.manifest.releaseId,
    "solarisael-house-proof",
    "constants.ts",
  );
  await writeFile(file, `${adapterSources["solarisael-house-proof/constants.ts"]} `);
  await expectRefusal(tree, "artifact solarisael-house-proof/constants.ts size mismatch");
});

test("refuses same-size artifact hash tampering before import", async () => {
  const tree = await makeInstalledTree();
  const file = path.join(tree.componentRoot, "versions", tree.manifest.releaseId, "index.ts");
  const tampered = await readFile(file);
  tampered[0] = tampered[0]! ^ 1;
  await writeFile(file, tampered);
  await expectRefusal(tree, "artifact index.ts SHA-256 mismatch");
});

test("recomputes canonical identity and refuses a releaseId mismatch before import", async () => {
  const tree = await makeInstalledTree();
  tree.manifest.version = "0.9.4";
  await persistManifest(tree);
  await expectRefusal(tree, "OMP adapter releaseId mismatch");
});

test("refuses every native compatibility mismatch before import", async () => {
  for (const field of ["hostApi", "substrateApi", "deliveryApi", "schemaVersion"] as const) {
    const tree = await makeInstalledTree();
    tree.manifest.compatibility[field] = Number(tree.manifest.compatibility[field]) + 1;
    await persistManifest(tree);
    await expectRefusal(tree, `OMP adapter ${field} mismatch`);
  }
});

test("fresh processes refuse non-string native pointer fields without coercion", async () => {
  for (const [field, value] of [
    ["version", 1],
    ["version", true],
    ["previousVersion", 1],
    ["previousVersion", false],
    ["rollbackBackup", 1],
    ["rollbackBackup", false],
  ] as const) {
    const tree = await makeInstalledTree();
    tree.nativePointer[field] = value;
    await persistNativePointer(tree);
    await expectFreshProcessRefusal(tree, /current release (?:version|previousVersion|rollbackBackup)/);
  }
});

test("native current pointer rejects fields outside its persisted contract", async () => {
  const tree = await makeInstalledTree();
  tree.nativePointer.unexpected = true;
  await persistNativePointer(tree);
  await expectRefusal(tree, "current release pointer has unknown field unexpected");
});

test("a fresh process refuses a non-Windows native release manifest", async () => {
  const tree = await makeInstalledTree();
  tree.nativeManifest.platform = "linux-x64";
  await persistNativeManifest(tree);
  await expectFreshProcessRefusal(tree, "active native release manifest platform is unsupported");
});

test("native and component versions share one grammar corpus", async () => {
  const validVersions = [
    "0",
    "a",
    "A1",
    "1.2.3",
    "1.2.3-rc.1+build.9",
    "a".repeat(128),
  ];
  for (const version of validVersions) {
    const tree = await makeInstalledTree(null, version, version);
    expect(configureInstalledAthanor({
      programRoot: tree.program,
      userProfile: tree.profile,
      env: tree.env,
    }).releaseId).toBe(tree.manifest.releaseId);
  }

  const invalidVersions = [
    "",
    " ",
    ".1.0.0",
    "-1.0.0",
    "+1.0.0",
    "../evil",
    "1.0.0/2",
    "1.0.0\\2",
    "1_0_0",
    "1..0",
    "1.0.0:1",
    "é1.0.0",
    "9".repeat(129),
  ];
  for (const version of invalidVersions) {
    const nativeTree = await makeInstalledTree();
    nativeTree.nativePointer.version = version;
    await persistNativePointer(nativeTree);
    await expectRefusal(nativeTree, "current release version is unsafe");

    const componentTree = await makeInstalledTree();
    componentTree.manifest.version = version;
    await persistManifest(componentTree);
    await expectRefusal(componentTree, "OMP adapter component version is unsafe");
  }
});

test("refuses a component artifact that is a symbolic link to an external file", async () => {
  const tree = await makeInstalledTree();
  const artifact = path.join(tree.releaseRoot, "index.ts");
  const external = path.join(tree.root, "external-index.ts");
  await writeFile(external, adapterSources["index.ts"]);
  await rm(artifact);
  await symlink(external, artifact, "file");

  await expectRefusal(tree, "artifact index.ts must be a regular physical file");
});

test("refuses a component artifact reached through an external directory junction", async () => {
  const tree = await makeInstalledTree();
  const ancestor = path.join(tree.releaseRoot, "solarisael-house-proof");
  const external = path.join(tree.root, "external-artifact-directory");
  await rename(ancestor, external);
  await symlink(external, ancestor, "junction");

  await expectRefusal(tree, "ancestor solarisael-house-proof must be a physical directory");
});

test("does not fall back to product-version adapter paths", async () => {
  const tree = await makeInstalledTree();
  await rm(tree.componentRoot, { recursive: true, force: true });
  const oldAdapter = path.join(tree.nativeRoot, "adapters", "omp");
  await mkdir(oldAdapter, { recursive: true });
  const oldModule = [
    "const state = globalThis as typeof globalThis & { __installedLoaderImports?: string[] };",
    "(state.__installedLoaderImports ??= []).push('old-product-adapter');",
    "export default async function oldAdapter() {}",
    "",
  ].join("\n");
  await writeFile(path.join(oldAdapter, "index.ts"), oldModule);
  await writeFile(path.join(oldAdapter, "hygiene.ts"), oldModule);

  await expectRefusal(tree, "component root ancestor omp-adapter could not be inspected");
});

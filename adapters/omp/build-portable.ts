// Builds the two public Athanor archives.
//
//   the-athanor-<version>-<platform>-vault.zip
//   the-athanor-<version>-<platform>-akasha.zip
//
// Both stage the same unified tree: an immutable `the-athanor/` product
// directory with the OMP adapter at `the-athanor/adapters/omp`. AKASHA adds the
// substrate operations and the platform substrate binary; Vault carries neither,
// and carries no PostgreSQL, embedding, WSL, or Rust runtime asset at all.
//
// One run produces every requested profile for the HOST platform only: the
// compiled installer/updater and the substrate binary are host artifacts.

import { createHash } from "node:crypto";
import { createReadStream } from "node:fs";
import { cp, mkdir, mkdtemp, readdir, rm, stat, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { spawn } from "node:child_process";
import { discoverRustExecutable } from "./discovery.ts";
import { ADAPTER_ROOT, ATHANOR_ROOT } from "./athanor-root.ts";
import {
  ADAPTER_RELATIVE,
  PRODUCT_DIRECTORY,
  PROFILES,
  SUBSTRATE_RELATIVE,
  archiveName,
  archivePlatform,
  isProfile,
  substrateBinaryRelative,
  type Profile,
} from "./install-layout.ts";

type ArchiveResult = { profile: Profile; path: string; sha256: string; size: number };

const packageMetadata = await Bun.file(path.join(ADAPTER_ROOT, "package.json")).json() as { version?: string };
const productVersion = String(packageMetadata.version || "").trim();
if (!/^\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$/.test(productVersion)) {
  throw new Error("package.json must contain a semantic product version");
}

const platform = archivePlatform();
if (!platform) throw new Error(`unsupported build platform: ${process.platform}-${process.arch}`);

function flag(name: string): string | null {
  const index = process.argv.indexOf(name);
  return index >= 0 ? process.argv[index + 1] ?? null : null;
}

const profileArgument = (flag("--profile") || "all").toLowerCase();
if (profileArgument !== "all" && !isProfile(profileArgument)) {
  throw new Error(`--profile must be vault, akasha, or all (got ${profileArgument})`);
}
const requestedProfiles: Profile[] = profileArgument === "all" ? [...PROFILES] : [profileArgument as Profile];
const outputDirectory = path.resolve(flag("--out-dir") || path.join(ADAPTER_ROOT, "dist"));

// Files copied verbatim from the product root into `the-athanor/`.
const PRODUCT_FILES = ["index.ts", "package.json", "README.md", "LICENSE", "NOTICE"];
// Onboarding surface at the archive root — the door an operator or AI opens first.
const ONBOARDING_FILES = ["README.md", "INSTALL.md", "USAGE.md", "IDENTITY_GUIDE.md", "LICENSE", "NOTICE"];
// Adapter modules copied into `the-athanor/adapters/omp/`.
const ADAPTER_FILES = [
  "index.ts",
  "athanor-root.ts",
  "install-layout.ts",
  "install-legacy.ts",
  "discovery.ts",
  "harnesses.ts",
  "giga.ts",
  "kitten-lineage.ts",
  "rust-transport.ts",
  "gui-server.ts",
  "hygiene.ts",
  "installer.ts",
  "updater.ts",
  "verify-install.ts",
  "package.json",
  "README.md",
  "LICENSE",
  "NOTICE",
];

const setup = (profile: Profile) => `The Athanor — ${profile === "akasha" ? "AKASHA" : "Vault"} bundle for OMP

Give README.md and this extracted bundle to a tool-capable AI and ask:

  Install The Athanor with me. Preserve my existing configuration and rooms,
  explain consequential changes, and guide the first-room session.

INSTALL.md is the complete installation and verification protocol.
IDENTITY_GUIDE.md explains how to co-author a room identity without copying the
fictional example in the-athanor/adapters/omp/starter-room/example.

Installing lays down exactly one product directory, the-athanor, beside your
rooms/ and state/ directories. Nothing outside the-athanor/ is overwritten.

${profile === "akasha"
  ? `This AKASHA bundle carries the substrate operations and the ${platform}
substrate binary. It needs PostgreSQL and an embedding endpoint you control.`
  : `This Vault bundle carries no substrate binary, no PostgreSQL, no embeddings,
no WSL, and no Rust runtime. It is file-attributed retrieval only.`}

This bundle contains no private rooms, memories, credentials, or substrate data.
`;

/**
 * stdout belongs to the result JSON alone, so a release workflow can pipe it
 * straight into jq. Child chatter — `bun build --compile`, `tar` — is inherited
 * onto stderr instead, where it stays visible in CI logs without corrupting the
 * machine-readable stream.
 */
function run(command: string, args: string[], cwd: string): Promise<void> {
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, { cwd, stdio: ["ignore", 2, 2], windowsHide: true });
    child.on("error", reject);
    child.on("exit", (code) => code === 0
      ? resolve()
      : reject(new Error(`${command} exited with code ${code}`)));
  });
}

async function fileSha256(file: string): Promise<string> {
  const hash = createHash("sha256");
  for await (const chunk of createReadStream(file)) hash.update(chunk);
  return hash.digest("hex");
}

/** Nothing generated, cached, or credential-bearing may enter an archive. */
function excluded(source: string): boolean {
  const name = path.basename(source);
  return name === "__pycache__"
    || name === ".pytest_cache"
    || name === "node_modules"
    || name === ".venv"
    || name === "dist"
    || name === ".env"
    || name.endsWith(".egg-info")
    || name.endsWith(".dump");
}

async function stageSubstrate(root: string): Promise<void> {
  await cp(path.join(ATHANOR_ROOT, "substrate"), path.join(root, SUBSTRATE_RELATIVE), {
    recursive: true,
    filter: (source) => !excluded(source),
  });
}

async function stageSubstrateBinary(root: string): Promise<string> {
  const executable = discoverRustExecutable({ env: process.env, moduleDir: ADAPTER_ROOT });
  if (!executable) {
    throw new Error(`No ${platform} substrate executable found; set ATHANOR_SUBSTRATE_EXE or ATHANOR_AUTO=1.`);
  }
  const relative = substrateBinaryRelative(platform as NonNullable<typeof platform>);
  const destination = path.join(root, ADAPTER_RELATIVE, relative);
  await mkdir(path.dirname(destination), { recursive: true });
  await cp(executable, destination);
  const details = await stat(destination);
  // No `version` field: nothing read it and nothing validated it. The manifest
  // describes artifacts, and the artifacts are what the verifier hashes.
  await writeFile(path.join(root, ADAPTER_RELATIVE, "rust-manifest.json"), JSON.stringify({
    artifacts: [{
      platform,
      path: relative,
      sha256: await fileSha256(destination),
      size: details.size,
    }],
  }, null, 2) + "\n", "utf8");
  return relative;
}

async function compileExecutable(root: string, source: string, stem: string): Promise<string> {
  const name = process.platform === "win32" ? `${stem}.exe` : stem;
  await run(process.execPath, [
    "build",
    path.join(ADAPTER_ROOT, source),
    "--compile",
    "--outfile",
    path.join(root, ADAPTER_RELATIVE, name),
  ], ADAPTER_ROOT);
  return name;
}

async function stagedArtifacts(root: string): Promise<Array<{ path: string; sha256: string; size: number }>> {
  const artifacts: Array<{ path: string; sha256: string; size: number }> = [];
  const walk = async (directory: string): Promise<void> => {
    for (const entry of await readdir(directory, { withFileTypes: true })) {
      const file = path.join(directory, entry.name);
      if (entry.isDirectory()) await walk(file);
      else if (entry.isFile()) {
        artifacts.push({
          path: path.relative(root, file).replaceAll("\\", "/"),
          sha256: await fileSha256(file),
          size: (await stat(file)).size,
        });
      }
    }
  };
  await walk(root);
  return artifacts.sort((left, right) => left.path.localeCompare(right.path));
}

async function writePackageManifest(root: string, profile: Profile, installer: string, updater: string, substrate: string | null): Promise<void> {
  await writeFile(path.join(root, ADAPTER_RELATIVE, "package-manifest.json"), JSON.stringify({
    schemaVersion: 3,
    productVersion,
    profile,
    platform,
    productDirectory: PRODUCT_DIRECTORY,
    adapterDirectory: ADAPTER_RELATIVE,
    installer,
    updater,
    supportedHarnesses: ["omp"],
    requiredSchemaVersion: 14,
    substrateBinary: substrate,
    artifacts: await stagedArtifacts(root),
  }, null, 2) + "\n", "utf8");
}

async function buildProfile(profile: Profile): Promise<ArchiveResult> {
  const parent = await mkdtemp(path.join(os.tmpdir(), `the-athanor-${profile}-`));
  const root = path.join(parent, "bundle");
  const product = path.join(root, PRODUCT_DIRECTORY);
  const adapter = path.join(root, ADAPTER_RELATIVE);
  const output = path.join(outputDirectory, archiveName(productVersion, platform as NonNullable<typeof platform>, profile));
  try {
    await mkdir(adapter, { recursive: true });
    await mkdir(outputDirectory, { recursive: true });

    await cp(path.join(ATHANOR_ROOT, "src"), path.join(product, "src"), {
      recursive: true,
      filter: (source) => !excluded(source),
    });
    for (const filename of PRODUCT_FILES) await cp(path.join(ATHANOR_ROOT, filename), path.join(product, filename));
    for (const filename of ONBOARDING_FILES) await cp(path.join(ATHANOR_ROOT, filename), path.join(root, filename));

    for (const filename of ADAPTER_FILES) await cp(path.join(ADAPTER_ROOT, filename), path.join(adapter, filename));
    await cp(path.join(ADAPTER_ROOT, "solarisael-house-proof"), path.join(adapter, "solarisael-house-proof"), { recursive: true, filter: (source) => !excluded(source) });
    await cp(path.join(ADAPTER_ROOT, "gui"), path.join(adapter, "gui"), { recursive: true });
    await cp(path.join(ADAPTER_ROOT, "starter-room"), path.join(adapter, "starter-room"), { recursive: true });
    await cp(path.join(ADAPTER_ROOT, "commands"), path.join(adapter, "commands"), { recursive: true })
      .catch((error) => { if (error?.code !== "ENOENT") throw error; });

    const substrate = profile === "akasha" ? await (async () => {
      await stageSubstrate(root);
      return await stageSubstrateBinary(root);
    })() : null;

    const installer = await compileExecutable(root, "installer.ts", "install");
    const updater = await compileExecutable(root, "updater.ts", "update");
    await writeFile(path.join(root, "SETUP.txt"), setup(profile), "utf8");
    await writePackageManifest(root, profile, installer, updater, substrate);

    await rm(output, { force: true });
    await run("tar", ["-a", "-c", "-f", output, "-C", root, "."], ADAPTER_ROOT);
    return { profile, path: output, sha256: await fileSha256(output), size: (await stat(output)).size };
  } finally {
    await rm(parent, { recursive: true, force: true });
  }
}

const archives: ArchiveResult[] = [];
for (const profile of requestedProfiles) archives.push(await buildProfile(profile));
console.log(JSON.stringify({ version: productVersion, platform, archives }, null, 2));

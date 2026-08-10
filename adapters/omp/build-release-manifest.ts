// Writes the single release manifest that describes every public archive of one
// Athanor release. Profile and platform are read out of each archive's
// canonical filename, so one final job can assemble the whole matrix without
// per-platform manifests or a merge step.
//
//   bun run build-release-manifest.ts \
//     --version 0.11.0 --output release-manifest.json \
//     --channel stable --repository solarisael/the-athanor \
//     --required-schema 13 --asset dist/the-athanor-0.11.0-windows-x64-vault.zip ...

import { createHash } from "node:crypto";
import { createReadStream } from "node:fs";
import { stat, writeFile } from "node:fs/promises";
import path from "node:path";
import { PROFILES, parseArchiveName, type ArchivePlatform, type Profile } from "./install-layout.ts";

type Channel = "stable" | "beta" | "experimental";
type ManifestAsset = {
  profile: Profile;
  platform: ArchivePlatform;
  name: string;
  sha256: string;
  size: number;
};

const usage = (): never => {
  throw new Error("Usage: build-release-manifest.ts --version VERSION --asset PATH [--asset PATH ...] [--output FILE] [--channel stable|beta|experimental] [--repository OWNER/REPO] [--required-schema N]");
};

const values = new Map<string, string>();
const assetArguments: string[] = [];
const argv = process.argv.slice(2);
for (let index = 0; index < argv.length; index++) {
  const argument = argv[index] as string;
  if (!["--version", "--asset", "--output", "--channel", "--repository", "--required-schema"].includes(argument)) usage();
  const value = argv[++index];
  if (!value || value.startsWith("--")) usage();
  if (argument === "--asset") assetArguments.push(value);
  else values.set(argument, value);
}

const versionArgument = values.get("--version");
if (!versionArgument || assetArguments.length === 0) usage();
const output = path.resolve(values.get("--output") || "release-manifest.json");
const channel = (values.get("--channel") || "stable") as Channel;
const repository = values.get("--repository") || "solarisael/the-athanor";
const requiredSchemaVersion = Number(values.get("--required-schema") || "14");

if (!["stable", "beta", "experimental"].includes(channel)) usage();
if (!/^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/.test(repository)) throw new Error("--repository must be OWNER/REPO");
if (!Number.isSafeInteger(requiredSchemaVersion) || requiredSchemaVersion < 0) {
  throw new Error("--required-schema must be a non-negative integer");
}

const version = (versionArgument as string).replace(/^v/, "");
if (!/^\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$/.test(version)) throw new Error(`invalid semantic version: ${versionArgument}`);
const prerelease = version.split("-", 2)[1] || "";
if (channel === "stable" && prerelease) throw new Error("stable releases cannot use a prerelease version");
if (channel === "beta" && !/^beta(?:[.+-]|$)/i.test(prerelease)) throw new Error("beta releases require a -beta version");
if (channel === "experimental" && !/^(?:experimental|exp)(?:[.+-]|$)/i.test(prerelease)) throw new Error("experimental releases require an -experimental or -exp version");

async function fileSha256(file: string): Promise<string> {
  const hash = createHash("sha256");
  for await (const chunk of createReadStream(file)) hash.update(chunk);
  return hash.digest("hex");
}

const assets: ManifestAsset[] = [];
const seen = new Set<string>();
for (const argument of assetArguments) {
  const file = path.resolve(argument);
  const name = path.basename(file);
  const parsed = parseArchiveName(name);
  if (!parsed) throw new Error(`asset is not a canonical Athanor archive name: ${name}`);
  if (parsed.version !== version) throw new Error(`asset ${name} is version ${parsed.version}, expected ${version}`);
  const key = `${parsed.profile}/${parsed.platform}`;
  if (seen.has(key)) throw new Error(`duplicate asset for ${key}`);
  seen.add(key);
  const details = await stat(file);
  if (!details.isFile()) throw new Error(`release asset is not a regular file: ${file}`);
  assets.push({
    profile: parsed.profile,
    platform: parsed.platform,
    name,
    sha256: await fileSha256(file),
    size: details.size,
  });
}

const missingProfiles = PROFILES.filter((profile) => !assets.some((asset) => asset.profile === profile));
if (missingProfiles.length) throw new Error(`release manifest is missing every asset for: ${missingProfiles.join(", ")}`);

assets.sort((left, right) => left.name.localeCompare(right.name));
await writeFile(output, JSON.stringify({
  schemaVersion: 2,
  version,
  tag: `v${version}`,
  channel,
  repository,
  requiredSchemaVersion,
  assets,
}, null, 2) + "\n", "utf8");
console.log(output);

import { createHash } from "node:crypto";
import { afterEach, describe, expect, test } from "bun:test";
import { mkdir, mkdtemp, readFile, rm, stat, writeFile } from "node:fs/promises";
import { spawn } from "node:child_process";
import os from "node:os";
import path from "node:path";
import { ADAPTER_RELATIVE, archiveName, archivePlatform, layout, type Profile } from "../install-layout.ts";

const adapterRoot = path.resolve(import.meta.dir, "..");
const updater = path.join(adapterRoot, "updater.ts");
const platform = archivePlatform();
if (!platform) throw new Error("updater tests require a supported archive platform");
const roots: string[] = [];
const servers: ReturnType<typeof Bun.serve>[] = [];

type CommandResult = { code: number; stdout: string; stderr: string };

function run(command: string, args: string[], cwd?: string): Promise<CommandResult> {
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, { cwd, windowsHide: true });
    let stdout = "";
    let stderr = "";
    child.stdout.on("data", (chunk) => stdout += chunk);
    child.stderr.on("data", (chunk) => stderr += chunk);
    let code = -1;
    child.on("error", reject);
    // `exit` can precede the final stdout/stderr chunks; assertions read both.
    child.on("exit", (value) => { code = value ?? -1; });
    child.on("close", () => resolve({ code, stdout, stderr }));
  });
}

async function releaseFixture(options: {
  currentVersion: string;
  availableVersion: string;
  profile?: Profile;
  installedProfile?: Profile;
  validBundle?: boolean;
  corruptDigest?: boolean;
  /** Publish only this profile's asset, to prove profile-specific selection. */
  onlyProfile?: Profile;
  /** Leave a 0.10.x product directory at the target. */
  legacyTarget?: boolean;
}) {
  const profile = options.profile ?? "vault";
  const root = await mkdtemp(path.join(os.tmpdir(), "athanor-updater-test-"));
  roots.push(root);
  const target = path.join(root, "installed");
  const config = path.join(root, "config.yml");
  const receipt = path.join(root, "receipt.json");
  const installedAdapter = layout(target).adapter;
  await mkdir(installedAdapter, { recursive: true });
  await writeFile(
    path.join(installedAdapter, "package-manifest.json"),
    JSON.stringify({ schemaVersion: 3, productVersion: options.currentVersion, profile: options.installedProfile ?? profile, platform }),
  );
  if (options.legacyTarget) await mkdir(path.join(target, "solarisael-house-omp"), { recursive: true });
  await writeFile(config, "extensions:\n");

  const assetName = archiveName(options.availableVersion, platform!, profile);
  const bundle = path.join(root, assetName);
  if (options.validBundle) {
    const tree = path.join(root, "release-tree");
    const packagedAdapter = path.join(tree, ADAPTER_RELATIVE);
    await mkdir(packagedAdapter, { recursive: true });
    const installerSource = path.join(root, "fixture-installer.ts");
    const installerName = process.platform === "win32" ? "install.exe" : "install";
    await writeFile(installerSource, "console.log(JSON.stringify({ok:true,args:process.argv.slice(1)}));\n");
    const compiled = await run(process.execPath, ["build", installerSource, "--compile", "--outfile", path.join(packagedAdapter, installerName)], root);
    if (compiled.code !== 0) throw new Error(`fixture installer compilation failed: ${compiled.stdout}${compiled.stderr}`);
    await writeFile(
      path.join(packagedAdapter, "package-manifest.json"),
      JSON.stringify({ schemaVersion: 3, productVersion: options.availableVersion, profile, platform, installer: installerName }),
    );
    const archived = await run("tar", ["-a", "-c", "-f", bundle, "-C", tree, "."], root);
    if (archived.code !== 0) throw new Error(`fixture archive failed: ${archived.stdout}${archived.stderr}`);
  } else {
    await writeFile(bundle, "local updater fixture");
  }

  const bytes = await readFile(bundle);
  const sha256 = options.corruptDigest ? "0".repeat(64) : createHash("sha256").update(bytes).digest("hex");
  const published: Profile[] = options.onlyProfile ? [options.onlyProfile] : ["vault", "akasha"];
  const manifest = {
    schemaVersion: 2,
    version: options.availableVersion,
    tag: `v${options.availableVersion}`,
    channel: "stable",
    repository: "solarisael/the-athanor",
    requiredSchemaVersion: 13,
    assets: published.map((candidate) => ({
      profile: candidate,
      platform,
      name: candidate === profile ? assetName : archiveName(options.availableVersion, platform!, candidate),
      sha256: candidate === profile ? sha256 : "1".repeat(64),
      size: candidate === profile ? bytes.byteLength : 1,
    })),
  };
  const server = Bun.serve({
    hostname: "127.0.0.1",
    port: 0,
    fetch(request) {
      const pathname = new URL(request.url).pathname;
      if (pathname === "/release-manifest.json") return Response.json(manifest);
      if (pathname === `/${assetName}`) return new Response(bytes);
      return new Response("not found", { status: 404 });
    },
  });
  servers.push(server);
  const manifestUrl = new URL("release-manifest.json", server.url).toString();
  const args = [
    "--target", target,
    "--room", "demo-room",
    "--mode", profile,
    "--config", config,
    "--manifest", manifestUrl,
    "--receipt", receipt,
  ];
  return { root, target, receipt, assetName, args };
}

afterEach(async () => {
  for (const server of servers.splice(0)) server.stop(true);
  await Promise.all(roots.splice(0).map((root) => rm(root, { recursive: true, force: true })));
});

describe.serial("release updater source", () => {
  test("defaults to the one public repository", async () => {
    const source = await readFile(updater, "utf8");
    expect(source).toContain('const DEFAULT_REPOSITORY = "solarisael/the-athanor"');
    expect(source).not.toContain("solarisael/solarisael-house-omp");
  });
});

describe.serial("release updater CLI", () => {
  for (const legacyMode of ["base", "full"] as const) {
    test(`--mode ${legacyMode} stops and points at the explicit migration`, async () => {
      const fixture = await releaseFixture({ currentVersion: "1.2.3", availableVersion: "1.3.0" });
      const args = fixture.args.map((value) => value === "vault" ? legacyMode : value);
      const result = await run(process.execPath, [updater, ...args], fixture.root);
      expect(result.code).not.toBe(0);
      expect(result.stderr).toContain("is a 0.10.x token");
      expect(result.stderr).toContain("--migrate-legacy");
    });
  }

  test("refuses to update a target that still holds a 0.10.x product directory", async () => {
    const fixture = await releaseFixture({ currentVersion: "1.2.3", availableVersion: "1.3.0", legacyTarget: true });
    const result = await run(process.execPath, [updater, ...fixture.args], fixture.root);
    expect(result.code).not.toBe(0);
    expect(result.stderr).toContain("0.10.x layout detected");
    expect(result.stderr).toContain("--migrate-legacy");
  });

  test("refuses to switch an installed profile", async () => {
    const fixture = await releaseFixture({
      currentVersion: "1.2.3",
      availableVersion: "1.3.0",
      profile: "vault",
      installedProfile: "akasha",
    });
    const result = await run(process.execPath, [updater, ...fixture.args], fixture.root);
    expect(result.code).not.toBe(0);
    expect(result.stderr).toContain("would change profiles");
  });
});

describe.serial("release updater resolution", () => {
  test("reads the installed version from the unified package manifest", async () => {
    const fixture = await releaseFixture({ currentVersion: "1.2.3", availableVersion: "1.2.3" });
    const result = await run(process.execPath, [updater, ...fixture.args], fixture.root);

    expect(result.code).toBe(0);
    expect(JSON.parse(result.stdout)).toMatchObject({
      ok: true,
      state: "current",
      profile: "vault",
      currentVersion: "1.2.3",
      availableVersion: "1.2.3",
      channel: "stable",
    });
  });

  test("reports an available newer release under the profile-specific asset name", async () => {
    const fixture = await releaseFixture({ currentVersion: "1.2.3", availableVersion: "1.3.0" });
    const result = await run(process.execPath, [updater, ...fixture.args, "--check"], fixture.root);

    expect(result.code).toBe(0);
    expect(JSON.parse(result.stdout)).toMatchObject({
      ok: true,
      state: "available",
      profile: "vault",
      asset: archiveName("1.3.0", platform!, "vault"),
    });
    expect(fixture.assetName).toBe(`the-athanor-1.3.0-${platform}-vault.zip`);
  });

  test("refuses a release that publishes no asset for the requested profile", async () => {
    const fixture = await releaseFixture({
      currentVersion: "1.2.3",
      availableVersion: "1.3.0",
      profile: "vault",
      onlyProfile: "akasha",
    });
    const result = await run(process.execPath, [updater, ...fixture.args, "--check"], fixture.root);
    expect(result.code).not.toBe(0);
    expect(result.stderr).toContain(`release has no vault asset for ${platform}`);
  });

  test("refuses a bundle whose streamed digest differs from the release manifest", async () => {
    const fixture = await releaseFixture({ currentVersion: "1.2.3", availableVersion: "1.3.0", corruptDigest: true });
    const installedManifest = path.join(layout(fixture.target).adapter, "package-manifest.json");
    const before = await readFile(installedManifest, "utf8");
    const result = await run(process.execPath, [updater, ...fixture.args], fixture.root);

    expect(result.code).not.toBe(0);
    expect(JSON.parse(result.stderr)).toMatchObject({ ok: false, state: "failed" });
    expect(result.stderr).toContain("bundle integrity mismatch");
    expect(await readFile(installedManifest, "utf8")).toBe(before);
    expect(await stat(fixture.receipt)).toBeDefined();
  });

  test("hands the profile and unified paths to the compiled installer", async () => {
    const fixture = await releaseFixture({ currentVersion: "1.2.3", availableVersion: "1.3.0", validBundle: true });
    const result = await run(process.execPath, [updater, ...fixture.args, "--harness", "omp", "--harness", "omp"], fixture.root);

    expect(result.code, JSON.stringify(result)).toBe(0);
    const output = JSON.parse(result.stdout);
    expect(output).toMatchObject({ ok: true, state: "updated", profile: "vault", previousVersion: "1.2.3", version: "1.3.0" });
    expect(output.installer.args).toEqual(expect.arrayContaining(["--update", "--harness", "omp", "--mode", "vault"]));
    expect(output.installer.args).not.toContain("--substrate");
    expect(output.installer.args.filter((value: string) => value === "--harness")).toHaveLength(1);
  }, 20_000);
});

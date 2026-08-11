import { afterEach, expect, test } from "bun:test";
import { mkdir, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { configureInstalledAthanor } from "../installed-loader.ts";

const roots: string[] = [];
afterEach(async () => {
  await Promise.all(roots.splice(0).map((root) => rm(root, { recursive: true, force: true })));
});

test("installed loader follows current.json and exposes room endpoints without writing secrets to module paths", async () => {
  const root = path.join(os.tmpdir(), `athanor-loader-${crypto.randomUUID()}`);
  roots.push(root);
  const program = path.join(root, "program");
  const profile = path.join(root, "operator");
  await mkdir(path.join(program, "versions", "1.0.0-rc.2", "adapters", "omp"), { recursive: true });
  await mkdir(path.join(profile, ".omp", "agent", "athanor"), { recursive: true });
  await writeFile(path.join(program, "current.json"), JSON.stringify({ version: "1.0.0-rc.2" }));
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
  const env: NodeJS.ProcessEnv = {};

  const modules = configureInstalledAthanor({ programRoot: program, userProfile: profile, env });

  expect(modules.index).toContain("1.0.0-rc.2/adapters/omp/index.ts");
  expect(modules.hygiene).toContain("1.0.0-rc.2/adapters/omp/hygiene.ts");
  expect(env.ATHANOR_HOST_HOUSE_ID).toBe("solarisael");
  expect(env.ATHANOR_HOST_TOKEN).toBe("private-token");
  expect(JSON.parse(env.ATHANOR_HOST_ENDPOINTS!)).toEqual({
    kintsu: { url: "ws://127.0.0.1:8787/athanor/v1/ws", spirit: "Kintsu" },
    kodo: { url: "ws://127.0.0.1:8788/athanor/v1/ws", spirit: "Kodo" },
  });
  expect(modules.index).not.toContain("private-token");
});

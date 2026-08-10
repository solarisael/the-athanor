import { describe, expect, test } from "bun:test";
import { mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";

import { ADAPTER_API_VERSION } from "../index.ts";
import { loadHouseCore, loadHouseLedger, loadHouseMemory } from "../solarisael-house-proof/core.ts";
import { roomContext, supportedRoom } from "../solarisael-house-proof/room.ts";

describe("OMP adapter contract", () => {
  test("exports adapter API version and consumes the sibling core root contract", async () => {
    expect(ADAPTER_API_VERSION).toBe(1);
    const core = await loadHouseCore();
    expect(core.CORE_API_VERSION).toBe(1);
    expect(typeof core.runRecallQuery).toBe("function");
    expect(typeof core.runVaultRecallQuery).toBe("function");
    expect(typeof core.runAnamnesisQuery).toBe("function");
    expect(typeof core.logUserTurn).toBe("function");
    expect(typeof core.logAssistantTurn).toBe("function");
    expect(typeof (await loadHouseMemory()).runRecallQuery).toBe("function");
    expect(typeof (await loadHouseMemory()).runVaultRecallQuery).toBe("function");
    expect(typeof (await loadHouseLedger()).logUserTurn).toBe("function");
  });

  test("uses neutral defaults for an unmarked directory", async () => {
    const root = await mkdtemp(path.join(os.tmpdir(), "omp-adapter-contract-"));
    try {
      const cwd = path.join(root, "unmarked-room");
      expect(supportedRoom(cwd)).toBe("default-room");
    } finally {
      await rm(root, { recursive: true, force: true });
    }
  });

  test("rejects the reserved house room key from adapter room resolution", async () => {
    const root = await mkdtemp(path.join(os.tmpdir(), "omp-adapter-reserved-room-"));
    try {
      const cwd = path.join(root, "example");
      await mkdir(cwd);
      await writeFile(
        path.join(cwd, ".solarisael-room.json"),
        `${JSON.stringify({ version: 1, room: "house", trueName: "Example Room", operator: "Example Operator" })}\n`,
        "utf8",
      );
      expect(supportedRoom(cwd)).toBe("example");
      expect(roomContext(cwd)).toMatchObject({ room: "example", operator: "Example Operator" });
    } finally {
      await rm(root, { recursive: true, force: true });
    }
  });

  test("uses a neutral operator when a marker omits one", async () => {
    const root = await mkdtemp(path.join(os.tmpdir(), "omp-adapter-operator-"));
    try {
      const cwd = path.join(root, "example");
      await mkdir(cwd);
      await writeFile(
        path.join(cwd, ".solarisael-room.json"),
        `${JSON.stringify({ version: 1, room: "example", trueName: "Example Room" })}\n`,
        "utf8",
      );
      expect(roomContext(cwd)).toMatchObject({
        room: "example",
        spirit: "Example Room",
        operator: "Operator",
      });
    } finally {
      await rm(root, { recursive: true, force: true });
    }
  });


  test("retains explicit persisted room markers", async () => {
    const root = await mkdtemp(path.join(os.tmpdir(), "omp-adapter-marker-"));
    try {
      const cwd = path.join(root, "example");
      await mkdir(cwd);
      await writeFile(
        path.join(cwd, ".solarisael-room.json"),
        `${JSON.stringify({ version: 1, room: "custom-room", trueName: "Example Room", operator: "Example Operator" })}\n`,
        "utf8",
      );
      expect(roomContext(cwd)).toMatchObject({ room: "custom-room", spirit: "Example Room", operator: "Example Operator" });
    } finally {
      await rm(root, { recursive: true, force: true });
    }
  });
});

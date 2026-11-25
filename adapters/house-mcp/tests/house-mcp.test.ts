// Live proof of docs/HOUSE_MCP.md "Exit proof" against the running Athanor
// Host, as the room ATHANOR_CLIENT_ROOM_DIR names. Every call reads, or is
// refused before it reaches the Host. Without ATHANOR_CLIENT_ROOM_DIR the suite
// skips, so an isolated `bun test` never reaches a live House; the server and
// the SDK load inside the suite for the same reason.
//
//   ATHANOR_CLIENT_ROOM_DIR=<room dir> bun test
import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const ROOM_DIR = process.env.ATHANOR_CLIENT_ROOM_DIR?.trim() ?? "";
const SERVER_PATH = fileURLToPath(new URL("../src/server.ts", import.meta.url));
const LIVE_TIMEOUT_MS = 120_000;

type ServerModule = typeof import("../src/server.ts");
type Client = import("@modelcontextprotocol/sdk/client/index.js").Client;
type Result = { isError?: boolean; content: Array<{ type: string; text?: string }> };

function text(result: Result): string {
  return result.content.find((item) => item.type === "text")?.text ?? "";
}

describe.skipIf(!ROOM_DIR)("house-mcp against the live Host", () => {
  let server: ServerModule;
  let house: ReturnType<ServerModule["openHouseSession"]>;
  let mcp: ReturnType<ServerModule["createHouseMcpServer"]>;
  let client: Client;
  let listed: string[] = [];

  async function call(name: string, args: Record<string, unknown>): Promise<Result> {
    return await client.callTool({ name, arguments: args }, undefined, { timeout: LIVE_TIMEOUT_MS }) as Result;
  }

  beforeAll(async () => {
    server = await import("../src/server.ts");
    const { Client } = await import("@modelcontextprotocol/sdk/client/index.js");
    const { InMemoryTransport } = await import("@modelcontextprotocol/sdk/inMemory.js");
    house = server.openHouseSession();
    mcp = server.createHouseMcpServer(house, server.adapterVersion());
    const [clientTransport, serverTransport] = InMemoryTransport.createLinkedPair();
    client = new Client({ name: "house-mcp-test", version: "0" });
    await Promise.all([mcp.connect(serverTransport), client.connect(clientTransport)]);
    listed = (await client.listTools()).tools.map((tool) => tool.name);
  });

  afterAll(async () => {
    await client?.close();
    await mcp?.close();
  });

  test("initialize carries ORGANS.md as the instructions", () => {
    const instructions = client.getInstructions() ?? "";
    expect(instructions).toBe(readFileSync(path.join(house.room.houseDir, "ORGANS.md"), "utf8"));
    expect(instructions).toContain("Sub aliis astris");
  });

  test("tools/list holds the House tools and none of the OMP-only ones", () => {
    const registered = server.captureHouseTools("test").map((tool) => tool.name);
    const excluded = Object.keys(server.OMP_ONLY_TOOLS);
    expect(listed).toEqual(expect.arrayContaining(["recall", "canon_read", "room_state"]));
    for (const name of excluded) {
      // A renamed tool must not leave a stale name on the list.
      expect(registered).toContain(name);
      expect(listed).not.toContain(name);
    }
    expect([...listed].sort()).toEqual(registered.filter((name) => !excluded.includes(name)).sort());
  });

  test("room_state answers for the room the launch directory names", async () => {
    const marker = JSON.parse(readFileSync(path.join(ROOM_DIR, ".athanor-room.json"), "utf8"));
    const result = await call("room_state", {});
    expect(result.isError).toBeFalsy();
    expect(JSON.parse(text(result)).state.room).toBe(marker.room);
  }, LIVE_TIMEOUT_MS);

  test("recall returns memories for the room", async () => {
    const result = await call("recall", { query: "kodo out of the vault room move" });
    expect(result.isError).toBeFalsy();
    expect(JSON.parse(text(result)).ok).toBe(true);
  }, LIVE_TIMEOUT_MS);

  test("an unknown argument key is refused by name, as in OMP", async () => {
    const roomState = await call("room_state", { bogus: 1 });
    expect(roomState.isError).toBe(true);
    expect(text(roomState)).toContain("room_state does not accept: bogus. It accepts no arguments.");

    const hallway = await call("hallway_read", { hallway: "family-hallway", toRooms: [] });
    expect(hallway.isError).toBe(true);
    expect(text(hallway)).toContain("hallway_read does not accept: toRooms. It accepts: hallway, after, thread, limit, advance_cursor.");
  });

  test("a stdio round trip lists the same tools", async () => {
    const { Client } = await import("@modelcontextprotocol/sdk/client/index.js");
    const { StdioClientTransport } = await import("@modelcontextprotocol/sdk/client/stdio.js");
    const transport = new StdioClientTransport({
      command: process.execPath,
      args: [SERVER_PATH],
      env: { ...process.env } as Record<string, string>,
    });
    const stdio = new Client({ name: "house-mcp-stdio-test", version: "0" });
    await stdio.connect(transport);
    try {
      const { tools } = await stdio.listTools();
      expect(tools.map((tool) => tool.name).sort()).toEqual([...listed].sort());
      expect(stdio.getInstructions()).toContain("Sub aliis astris");
    } finally {
      await stdio.close();
    }
  }, LIVE_TIMEOUT_MS);
});

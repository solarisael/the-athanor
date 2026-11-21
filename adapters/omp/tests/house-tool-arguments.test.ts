import { expect, test } from "bun:test";
import { z } from "zod";
import { registerSolarisaelTools } from "../house-proof/tools.ts";

// 2026-11-21, live: Hallway message 350 lost its recipients. The caller sent
// `toRooms`, the tool reads `to_rooms`, and the tool answered `ok: true`.
function registeredTool(name: string) {
  const tools = new Map<string, { execute: (...args: unknown[]) => Promise<any> }>();
  const pi = {
    zod: z,
    registerTool(definition: { name: string; execute: (...args: unknown[]) => Promise<any> }) {
      tools.set(definition.name, definition);
    },
    on() {},
    registerCommand() {},
    registerHook() {},
  };
  registerSolarisaelTools(pi, { version: "test" });
  const tool = tools.get(name);

  if (!tool) throw new Error(`${name} is not registered`);

  return tool;
}

test("a House tool refuses an unknown argument key and names the accepted keys", async () => {
  const result = await registeredTool("hallway_post").execute(
    "call-1",
    { hallway: "family-hallway", body: "hello", toRooms: ["tuner"] },
    undefined,
    undefined,
    undefined,
  );

  expect(result.isError).toBe(true);
  expect(result.details.ok).toBe(false);
  expect(result.details.error).toContain("does not accept: toRooms");
  expect(result.details.error).toContain("to_rooms");
});

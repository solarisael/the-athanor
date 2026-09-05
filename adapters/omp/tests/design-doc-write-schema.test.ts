import { describe, expect, test } from "bun:test";
import { z } from "zod";
import { registerSolarisaelTools } from "../house-proof/tools.ts";

// 2026-09-05, live: design document #24 landed with values {} and
// provenance {} because z.object({}) strips every unknown key. Rows #13-#24
// carry the same wound. Free-form JSON parameters must survive the schema.
function registeredParameters(name: string) {
  const tools = new Map<string, { parameters: z.ZodTypeAny }>();
  const pi = {
    zod: z,
    registerTool(definition: { name: string; parameters: z.ZodTypeAny }) {
      tools.set(definition.name, definition);
    },
    on() {},
    registerCommand() {},
    registerHook() {},
  };
  registerSolarisaelTools(pi, { version: "test" });
  const tool = tools.get(name);
  if (!tool) throw new Error(`${name} is not registered`);
  return tool.parameters;
}

describe("design_doc_write parameters", () => {
  test("values and provenance keep every key the caller sent", () => {
    const values = { "--ui_bg_root": "oklch(8.5% 0.005 280)", nested: { hue: 280 } };
    const provenance = { repo: "C:/Projects/solarisael", lines: "23-34", ruling: "memory 4392" };
    const parsed = registeredParameters("design_doc_write").parse({
      system: "solarisael",
      docType: "token",
      name: "reliquary-palette",
      values,
      provenance,
    });
    expect(parsed.values).toEqual(values);
    expect(parsed.provenance).toEqual(provenance);
  });

  test("absent values and provenance stay absent, not emptied", () => {
    const parsed = registeredParameters("design_doc_write").parse({
      system: "solarisael",
      docType: "guideline",
      name: "dark-first-cold-void",
    });
    expect(parsed.values).toBeUndefined();
    expect(parsed.provenance).toBeUndefined();
  });
});

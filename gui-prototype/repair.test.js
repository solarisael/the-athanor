import { afterEach, expect, test } from "bun:test";
import { queryRepairStatus, renderRepair } from "./repair.js";

const originalFetch = globalThis.fetch;
afterEach(() => { globalThis.fetch = originalFetch; });

const matrix = {
  ok: false,
  components: [{ name: "service", installed: true, running: false, reachable: null, healthy: false, detail: "Stopped <service>" }],
  missing: ["service"], elevationRequired: ["service"]
};

test("repair status renders each independent fact and explicit elevation action", async () => {
  globalThis.fetch = async () => Response.json(matrix);
  await queryRepairStatus();
  const html = renderRepair();
  expect(html).toContain("<strong>service</strong>");
  expect(html).toContain('title="installed: yes"');
  expect(html).toContain('title="running: no"');
  expect(html).toContain('title="reachable: not reported"');
  expect(html).toContain('title="healthy: no"');
  expect(html).toContain("Stopped &lt;service&gt;");
  expect(html).toContain("Start what is missing");
  expect(html).toContain("Start service (asks for administrator)");
  globalThis.fetch = async () => Response.json({ ...matrix, elevationRequired: [] });
  await queryRepairStatus();
  expect(renderRepair()).not.toContain("Start service (asks for administrator)");
});

test("repair hop retains the local executable error instead of blaming Host", async () => {
  globalThis.fetch = async () => Response.json({ error: "Repair exe missing; stderr: access denied", hop: "repair" }, { status: 502 });
  await queryRepairStatus();
  expect(renderRepair()).toContain("Repair exe missing; stderr: access denied");
});

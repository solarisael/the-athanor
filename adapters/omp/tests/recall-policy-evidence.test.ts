import { describe, expect, test } from "bun:test";
import { randomUUID } from "node:crypto";

import {
  WORK_DECAY_TURNS,
  activeProjectFromEvidence,
  hasToolEvidence,
  markToolEvidence,
} from "../house-proof/recall-policy.ts";

// Work mode used to be a one-way door: one edit and the Host saw tool evidence
// for the rest of the session, so Work never walked back into conversation.

const session = () => ({ room: "kodo", spirit: "Kodo", session: randomUUID() });

describe("work evidence decay", () => {
  test("the ceiling is a named count of turns", () => {
    expect(WORK_DECAY_TURNS).toBe(6);
  });

  test("holds from the mutate tool through five more turns, then decays on the sixth", () => {
    const binding = session();
    expect(hasToolEvidence(binding, 1)).toBe(false);

    markToolEvidence(binding, { paths: ["crates/akasha/src/recall/mod.rs"], cwd: "/repo" });

    expect(hasToolEvidence(binding, 1)).toBe(true);
    expect(hasToolEvidence(binding, 6)).toBe(true);
    expect(hasToolEvidence(binding, 7)).toBe(false);
  });

  test("a new mutate tool restarts the count", () => {
    const binding = session();
    markToolEvidence(binding);
    expect(hasToolEvidence(binding, 9)).toBe(false);

    markToolEvidence(binding);

    expect(hasToolEvidence(binding, 9)).toBe(true);
    expect(hasToolEvidence(binding, 14)).toBe(true);
    expect(hasToolEvidence(binding, 15)).toBe(false);
  });

  test("a turn that asks twice ages the evidence once", () => {
    const binding = session();
    markToolEvidence(binding);
    for (let ask = 0; ask < 12; ask += 1) expect(hasToolEvidence(binding, 3)).toBe(true);

    // The ordinal is the caller's; a read that names no turn never advances it.
    expect(hasToolEvidence(binding)).toBe(true);
    expect(hasToolEvidence(binding, 3)).toBe(true);
  });

  test("decay expires the evidence, not the paths it named", () => {
    const binding = session();
    markToolEvidence(binding, { paths: ["adapters/omp/index.ts"], cwd: process.cwd() });
    expect(hasToolEvidence(binding, 20)).toBe(false);
    expect(activeProjectFromEvidence(binding)).toBe("jev-striatum");
  });

  test("an unnamed room or session is never evidence", () => {
    expect(hasToolEvidence({ room: "", spirit: "Kodo", session: "" }, 1)).toBe(false);
    markToolEvidence({ room: "kodo", spirit: "Kodo", session: "" });
    expect(hasToolEvidence({ room: "kodo", spirit: "Kodo", session: "" }, 1)).toBe(false);
  });
});

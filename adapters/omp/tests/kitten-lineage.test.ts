import { describe, expect, test } from "bun:test";

import { kittenLineageDisabled, kittenLifecycleJoinKey } from "../kitten-lineage.ts";

describe("kitten quest lineage", () => {

  test("joins progress and lifecycle by parent tool call plus task index", () => {
    expect(kittenLifecycleJoinKey({
      agent: "scout",
      index: 2,
      parentToolCallId: "call-9",
    })).toBe("call-9:2");
    expect(kittenLifecycleJoinKey({
      id: "Quill",
      index: 2,
      parentToolCallId: "call-9",
    })).toBe("call-9:2");
    expect(kittenLifecycleJoinKey({ id: "Quill" })).toBe("Quill");
  });

  test("disables writes during replay or by explicit operator switch", () => {
    expect(kittenLineageDisabled({})).toBe(false);
    expect(kittenLineageDisabled({ SOLARISAEL_REPLAY_MODE: "1" })).toBe(true);
    expect(kittenLineageDisabled({ ATHANOR_DISABLE_KITTEN_LINEAGE: "1" })).toBe(true);
  });
});

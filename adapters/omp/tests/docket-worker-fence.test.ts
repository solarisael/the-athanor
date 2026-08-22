// Kills: a worker session writing to the Docket through the room's own
// injected capability (guild-hall #144; M1 criterion 4 settled NOT_MET on
// Kerf's probe receipt proving exactly this absence). The fence compares the
// caller's session against the room's embodied session registered at
// session_start, and it fails CLOSED: no registration means no writes.
// red-proof: make workerAtTheDoor return false when no embodiment exists,
// or compare against the caller's own session.

import { describe, expect, test } from "bun:test";
import {
  embodiedSession,
  registerEmbodiedSession,
} from "../solarisael-house-proof/host.ts";
import { workerAtTheDoor } from "../solarisael-house-proof/tools.ts";

describe("docket worker fence", () => {
  test("fails closed before any embodiment is registered", () => {
    expect(embodiedSession("fence-room-a")).toBeNull();
    expect(
      workerAtTheDoor({}, { room: "fence-room-a", session: "anyone" }),
    ).toBe(true);
  });

  test("the embodied session passes; every other session refuses", () => {
    registerEmbodiedSession("fence-room-b", "embodied-1");
    expect(embodiedSession("fence-room-b")).toBe("embodied-1");
    expect(
      workerAtTheDoor({}, { room: "fence-room-b", session: "embodied-1" }),
    ).toBe(false);
    expect(
      workerAtTheDoor({}, { room: "fence-room-b", session: "worker-7" }),
    ).toBe(true);
  });

  test("a session switch re-registers and retires the old embodiment", () => {
    registerEmbodiedSession("fence-room-c", "first-session");
    registerEmbodiedSession("fence-room-c", "second-session");
    expect(
      workerAtTheDoor({}, { room: "fence-room-c", session: "first-session" }),
    ).toBe(true);
    expect(
      workerAtTheDoor({}, { room: "fence-room-c", session: "second-session" }),
    ).toBe(false);
  });

  test("blank rooms and sessions never register", () => {
    registerEmbodiedSession("", "ghost");
    registerEmbodiedSession("fence-room-d", "  ");
    expect(embodiedSession("")).toBeNull();
    expect(embodiedSession("fence-room-d")).toBeNull();
  });
});

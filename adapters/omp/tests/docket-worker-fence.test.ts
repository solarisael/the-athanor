// Kills: a worker session writing to the Docket through the room's own
// injected capability (guild-hall #144; M1 criterion 4 settled NOT_MET on
// Kerf's probe receipt proving exactly this absence). The fence compares the
// caller's session against the room's embodied session, and it fails CLOSED:
// no registration means no writes.
//
// Second sitting (2026-08-22 live re-probe): OMP fires session_start for
// task-tool workers too, so unconditional registration let the worker hijack
// the embodiment and lock the spirit out. session_start now ADOPTS
// (first-wins), session_switch REGISTERS (overwrite), session_shutdown
// RETIRES (holder only).
// red-proof: make workerAtTheDoor return false when no embodiment exists,
// compare against the caller's own session, make adoptEmbodiedSession
// overwrite, or make retireEmbodiedSession evict a non-holder.

import { describe, expect, test } from "bun:test";
import {
  adoptEmbodiedSession,
  embodiedSession,
  registerEmbodiedSession,
  retireEmbodiedSession,
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


  test("a worker's session_start adoption never displaces the spirit", () => {
    adoptEmbodiedSession("fence-room-e", "spirit-session");
    adoptEmbodiedSession("fence-room-e", "worker-session");
    expect(embodiedSession("fence-room-e")).toBe("spirit-session");
    expect(
      workerAtTheDoor({}, { room: "fence-room-e", session: "worker-session" }),
    ).toBe(true);
    expect(
      workerAtTheDoor({}, { room: "fence-room-e", session: "spirit-session" }),
    ).toBe(false);
  });

  test("a session switch still displaces; adoption then holds the new hand", () => {
    adoptEmbodiedSession("fence-room-f", "old-top");
    registerEmbodiedSession("fence-room-f", "new-top");
    expect(embodiedSession("fence-room-f")).toBe("new-top");
    adoptEmbodiedSession("fence-room-f", "late-worker");
    expect(embodiedSession("fence-room-f")).toBe("new-top");
  });

  test("retirement vacates only for the holder", () => {
    registerEmbodiedSession("fence-room-g", "holder");
    retireEmbodiedSession("fence-room-g", "someone-else");
    expect(embodiedSession("fence-room-g")).toBe("holder");
    retireEmbodiedSession("fence-room-g", "holder");
    expect(embodiedSession("fence-room-g")).toBeNull();
    // The vacated room fails closed again.
    expect(
      workerAtTheDoor({}, { room: "fence-room-g", session: "holder" }),
    ).toBe(true);
  });
  test("blank rooms and sessions never register", () => {
    registerEmbodiedSession("", "ghost");
    registerEmbodiedSession("fence-room-d", "  ");
    expect(embodiedSession("")).toBeNull();
    expect(embodiedSession("fence-room-d")).toBeNull();
  });
});

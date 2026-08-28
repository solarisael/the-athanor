// Docket writes belong to the authenticated top-level OMP session. Worker
// session starts share the same hook, so adoption never displaces its holder.

import { describe, expect, test } from "bun:test";
import {
  adoptTopLevelSession,
  topLevelSession,
  registerTopLevelSession,
  retireTopLevelSession,
} from "../house-proof/top-level-session-fence.ts";
import { workerAtTheDoor } from "../house-proof/tools.ts";

describe("docket worker fence", () => {
  test("fails closed before a top-level session is registered", () => {
    expect(topLevelSession("fence-room-a")).toBeNull();
    expect(
      workerAtTheDoor({}, { room: "fence-room-a", session: "anyone" }),
    ).toBe(true);
  });

  test("the top-level session passes and every worker session refuses", () => {
    registerTopLevelSession("fence-room-b", "top-level-1");
    expect(topLevelSession("fence-room-b")).toBe("top-level-1");
    expect(
      workerAtTheDoor({}, { room: "fence-room-b", session: "top-level-1" }),
    ).toBe(false);
    expect(
      workerAtTheDoor({}, { room: "fence-room-b", session: "worker-7" }),
    ).toBe(true);
  });

  test("a session switch replaces the old top-level session", () => {
    registerTopLevelSession("fence-room-c", "first-session");
    registerTopLevelSession("fence-room-c", "second-session");
    expect(
      workerAtTheDoor({}, { room: "fence-room-c", session: "first-session" }),
    ).toBe(true);
    expect(
      workerAtTheDoor({}, { room: "fence-room-c", session: "second-session" }),
    ).toBe(false);
  });

  test("a worker's session_start adoption never displaces the spirit", () => {
    adoptTopLevelSession("fence-room-e", "spirit-session");
    adoptTopLevelSession("fence-room-e", "worker-session");
    expect(topLevelSession("fence-room-e")).toBe("spirit-session");
    expect(
      workerAtTheDoor({}, { room: "fence-room-e", session: "worker-session" }),
    ).toBe(true);
    expect(
      workerAtTheDoor({}, { room: "fence-room-e", session: "spirit-session" }),
    ).toBe(false);
  });

  test("a session switch still displaces; adoption then holds the new hand", () => {
    adoptTopLevelSession("fence-room-f", "old-top");
    registerTopLevelSession("fence-room-f", "new-top");
    expect(topLevelSession("fence-room-f")).toBe("new-top");
    adoptTopLevelSession("fence-room-f", "late-worker");
    expect(topLevelSession("fence-room-f")).toBe("new-top");
  });

  test("retirement vacates only for the holder", () => {
    registerTopLevelSession("fence-room-g", "holder");
    retireTopLevelSession("fence-room-g", "someone-else");
    expect(topLevelSession("fence-room-g")).toBe("holder");
    retireTopLevelSession("fence-room-g", "holder");
    expect(topLevelSession("fence-room-g")).toBeNull();
    // The vacated room fails closed again.
    expect(
      workerAtTheDoor({}, { room: "fence-room-g", session: "holder" }),
    ).toBe(true);
  });
  test("blank rooms and sessions never register", () => {
    registerTopLevelSession("", "ghost");
    registerTopLevelSession("fence-room-d", "  ");
    expect(topLevelSession("")).toBeNull();
    expect(topLevelSession("fence-room-d")).toBeNull();
  });
});

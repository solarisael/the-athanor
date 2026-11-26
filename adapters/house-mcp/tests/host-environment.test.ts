// resolveHostEnvironment with an injected environment and a temporary profile
// folder. No test here reads the real environment, client.json, or data root,
// so this file needs no Host. Each place holds its own values, so a key can
// only come from the place the order chose.
import { afterEach, beforeEach, expect, test } from "bun:test";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { resolveHostEnvironment, StartupRefusal } from "../src/server.ts";

let profile: string;

beforeEach(() => {
  profile = mkdtempSync(path.join(tmpdir(), "house-mcp-host-"));
});

afterEach(() => {
  rmSync(profile, { recursive: true, force: true });
});

function clientFile(): string {
  return path.join(profile, ".omp", "agent", "athanor", "client.json");
}

function homeDataRoot(): string {
  return path.join(profile, ".local", "state", "athanor");
}

function writeClientProjection(): void {
  mkdirSync(path.dirname(clientFile()), { recursive: true });
  writeFileSync(clientFile(), JSON.stringify({
    format: 2,
    stateRoot: "/client/state",
    houseId: "client-house",
    hostToken: "client-token",
    hostUrl: "ws://127.0.0.1:7001",
  }));
}

// The token file ends with a newline, as a file written by hand does.
function writeDataRoot(root: string, label: string, hostPort: number): void {
  mkdirSync(path.join(root, "config"), { recursive: true });
  writeFileSync(path.join(root, "host.token"), `${label}-token\n`);
  writeFileSync(path.join(root, "config", "runtime.json"), JSON.stringify({ hostPort, houseId: `${label}-house` }));
}

function refusal(env: NodeJS.ProcessEnv): StartupRefusal {
  try {
    resolveHostEnvironment(env, profile);
  } catch (error) {
    if (error instanceof StartupRefusal) return error;
    throw error;
  }
  throw new Error("resolveHostEnvironment did not refuse");
}

test("a key that is already set wins over client.json and the data root", () => {
  writeClientProjection();
  writeDataRoot(homeDataRoot(), "data-root", 7002);
  const env: NodeJS.ProcessEnv = {
    ATHANOR_HOST_TOKEN: "env-token",
    ATHANOR_HOST_HOUSE_ID: "env-house",
    ATHANOR_HOST_URL: "ws://127.0.0.1:7000",
  };

  const result = resolveHostEnvironment(env, profile);

  expect(result.set).toEqual(["ATHANOR_STATE_DIR"]);
  expect(env).toEqual({
    ATHANOR_HOST_TOKEN: "env-token",
    ATHANOR_HOST_HOUSE_ID: "env-house",
    ATHANOR_HOST_URL: "ws://127.0.0.1:7000",
    ATHANOR_STATE_DIR: "/client/state",
  });
});

test("client.json comes next, before the data root", () => {
  writeClientProjection();
  writeDataRoot(homeDataRoot(), "data-root", 7002);
  const env: NodeJS.ProcessEnv = {};

  const result = resolveHostEnvironment(env, profile);

  expect(result.source).toBe(clientFile());
  expect(env).toEqual({
    ATHANOR_STATE_DIR: "/client/state",
    ATHANOR_HOST_HOUSE_ID: "client-house",
    ATHANOR_HOST_TOKEN: "client-token",
    ATHANOR_HOST_URL: "ws://127.0.0.1:7001",
  });
});

test("without client.json, the data root gives the token, the House id, and the URL", () => {
  writeDataRoot(homeDataRoot(), "data-root", 7002);
  const env: NodeJS.ProcessEnv = {};

  const result = resolveHostEnvironment(env, profile);

  expect(result.source).toBe(homeDataRoot());
  expect(env).toEqual({
    ATHANOR_HOST_TOKEN: "data-root-token",
    ATHANOR_HOST_HOUSE_ID: "data-root-house",
    ATHANOR_HOST_URL: "ws://127.0.0.1:7002",
  });
});

test("the data root is $ATHANOR_DATA_ROOT, else $XDG_STATE_HOME/athanor, else ~/.local/state/athanor", () => {
  const explicit = path.join(profile, "explicit");
  const stateHome = path.join(profile, "xdg-state");
  writeDataRoot(explicit, "explicit", 7003);
  writeDataRoot(path.join(stateHome, "athanor"), "xdg", 7004);
  writeDataRoot(homeDataRoot(), "home", 7005);
  const cases: Array<[NodeJS.ProcessEnv, string]> = [
    [{ ATHANOR_DATA_ROOT: explicit, XDG_STATE_HOME: stateHome }, "explicit-token"],
    [{ XDG_STATE_HOME: stateHome }, "xdg-token"],
    [{}, "home-token"],
  ];

  for (const [env, token] of cases) {
    resolveHostEnvironment(env, profile);
    expect(env.ATHANOR_HOST_TOKEN).toBe(token);
  }
});

test("a key that no place holds stops the start, and the refusal names each place", () => {
  const nothing = refusal({});

  expect(nothing.message).toStartWith("no ATHANOR_HOST_TOKEN and no ATHANOR_HOST_HOUSE_ID.");
  expect(nothing.message).toContain(clientFile());
  expect(nothing.message).toContain(path.join(homeDataRoot(), "host.token"));
  expect(nothing.message).toContain(path.join(homeDataRoot(), "config", "runtime.json"));

  mkdirSync(homeDataRoot(), { recursive: true });
  writeFileSync(path.join(homeDataRoot(), "host.token"), "data-root-token");

  expect(refusal({}).message).toStartWith("no ATHANOR_HOST_HOUSE_ID.");
});

import { expect, test } from "bun:test";
import { settleAutomaticContextWithinBudget } from "../index.ts";
import { AUTOMATIC_CONTEXT_IO_TIMEOUT_MS } from "../house-proof/constants.ts";

test("automatic context uses at most a five-second Windows budget", () => {
  if (process.platform === "win32") expect(AUTOMATIC_CONTEXT_IO_TIMEOUT_MS).toBe(5_000);
  else expect(AUTOMATIC_CONTEXT_IO_TIMEOUT_MS).toBeLessThanOrEqual(5_000);
});

test("automatic context returns settled work", async () => {
  expect(await settleAutomaticContextWithinBudget(Promise.resolve("ready"), 25)).toEqual({
    status: "settled",
    value: "ready",
  });
});

test("automatic context contains rejected work", async () => {
  const error = new Error("database unavailable");
  expect(await settleAutomaticContextWithinBudget(Promise.reject(error), 25)).toEqual({
    status: "failed",
    error,
  });
});

test("automatic context releases the turn at its deadline", async () => {
  const started = performance.now();
  const result = await settleAutomaticContextWithinBudget(new Promise<never>(() => undefined), 10);
  expect(result).toEqual({ status: "timeout" });
  expect(performance.now() - started).toBeLessThan(250);
});

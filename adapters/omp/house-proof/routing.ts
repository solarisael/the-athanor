// OMP client for the Athanor Host routing family.
// Lane status, spellbook resolution, page writes, and the one-selector dispatch
// decision all happen in Rust; this module only carries the command and returns
// the receipt. The Host knows its own room; no caller names it.

import { hostCommand, sendHostCommand, type HostBinding } from "./host.ts";

const ROUTING_STATUS = "athanor.routing.status";
const ROUTING_DISPATCH = "athanor.routing.dispatch";
const FAMILIAR_STATUS = "athanor.familiar.status";
const SPELLBOOK_WRITE = "athanor.spellbook.write";
const ROUTING_RESULT = "athanor.routing.result";
const ACCEPTED = new Set([ROUTING_RESULT]);

async function request(
  binding: HostBinding,
  commandType: string,
  payload: Record<string, unknown> = {},
): Promise<Record<string, any>> {
  const command = hostCommand(binding, commandType, "routing", payload);
  const response = await sendHostCommand(command, ACCEPTED);
  if (!response.result || typeof response.result !== "object") {
    throw new Error("Athanor Host routing response omitted result");
  }
  return response.result;
}

export async function laneStatus(binding: HostBinding) {
  return await request(binding, ROUTING_STATUS);
}

export async function familiarStatus(binding: HostBinding) {
  return await request(binding, FAMILIAR_STATUS);
}

export async function dispatchHouse(binding: HostBinding, params: unknown) {
  return await request(binding, ROUTING_DISPATCH, {
    routing_request: params && typeof params === "object" ? params : {},
  });
}

export async function writeSpellbook(binding: HostBinding, scope: string, page: unknown) {
  return await request(binding, SPELLBOOK_WRITE, { routing_request: { scope, page } });
}

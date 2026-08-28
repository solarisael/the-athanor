// OMP client for the Athanor Host routing family.
// Lane status, spellbook resolution, and the one-selector dispatch decision all
// happen in Rust; this module only carries the command and returns the receipt.

import { hostCommand, sendHostCommand, type HostBinding } from "./host.ts";

const ROUTING_STATUS = "athanor.routing.status";
const ROUTING_DISPATCH = "athanor.routing.dispatch";
const FAMILIAR_STATUS = "athanor.familiar.status";
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

export async function familiarStatus(binding: HostBinding, roomDir: string) {
  return await request(binding, FAMILIAR_STATUS, { room_dir: roomDir });
}

export async function dispatchHouse(binding: HostBinding, roomDir: string, params: unknown) {
  return await request(binding, ROUTING_DISPATCH, {
    room_dir: roomDir,
    routing_request: params && typeof params === "object" ? params : {},
  });
}

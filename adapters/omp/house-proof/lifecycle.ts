import { hostCommand, sendHostCommand, type HostBinding } from "./host.ts";

export async function lifecyclePlan<T>(binding: HostBinding, request: Record<string, unknown>): Promise<T> {
  const response = await sendHostCommand(
    hostCommand(binding, "athanor.lifecycle.plan", "lifecycle", { lifecycle_request: request }),
    new Set(["athanor.lifecycle.result"]),
  );
  return response.result as T;
}

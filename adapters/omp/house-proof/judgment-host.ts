import type { HostBinding } from "./host.ts";

export async function resolveJudgmentCredential(
  context: any,
  binding: HostBinding,
  signal?: AbortSignal,
): Promise<string | undefined> {
  const value = await context?.modelRegistry?.authStorage?.getApiKey?.(
    "typesafe",
    binding.session,
    signal ? { signal } : undefined,
  );
  return typeof value === "string" && value.length > 0 ? value : undefined;
}

// Provider payload hooks run after OMP has placed its own cache markers. Keep
// its lifetime and end-of-request anchor, but pin the other message anchor to
// the last complete four-line view block instead of the changing partial tail.
export function markViewCache(payload: unknown, anchor: string | null): { changed: boolean; reason?: string } {
  if (!anchor) return { changed: false };
  if (!payload || typeof payload !== "object" || !Array.isArray((payload as any).messages)) {
    return { changed: false, reason: "the provider payload has no supported message array" };
  }
  const wire = payload as any;
  const blocks: any[] = wire.messages.flatMap((message: any) => Array.isArray(message?.content) ? message.content : []);
  const target = blocks.find((block) => block?.type === "text" && block.text === anchor);
  if (!target) return { changed: false, reason: "the provider did not preserve the whole view block" };
  const marked = blocks.filter((block) => block?.cache_control?.type === "ephemeral");
  const end = marked.at(-1);
  // Respect an operator/provider with caching disabled; do not enable it here.
  if (!end) return { changed: false, reason: "provider message caching is not enabled" };
  const heads = [wire.system, wire.tools].reduce((count: number, values: unknown) => count
    + (Array.isArray(values) ? values.filter((value) => value?.cache_control !== undefined).length : 0), 0);
  if (heads + (target === end ? 1 : 2) > 4) {
    return { changed: false, reason: "the provider head consumes the four-marker budget" };
  }
  const control = { ...end.cache_control };
  for (const block of marked) delete block.cache_control;
  target.cache_control = { ...control };
  end.cache_control = control;
  return { changed: true };
}

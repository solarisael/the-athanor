import type { WorkerTaskOrigin } from "../worker-lifecycle.ts";

type Origin = { batch: string; agent: string; index: number };
type Projection = { batch: string; ids: string[]; running: number; jobs: any[] };

export function compactGuMessages(messages: any[], enrolled: readonly WorkerTaskOrigin[] = []): any[] {
  const origins = collectOrigins(messages, enrolled);
  const projected: any[] = [];
  let changed = false;

  for (let index = 0; index < messages.length; index++) {
    const message = messages[index];
    const success = successProjection(message, index, origins);

    if (success) {
      let end = index + 1;

      while (end < messages.length) {
        const next = successProjection(messages[end], end, origins);

        if (!next || next.batch !== success.batch) break;

        success.ids.push(...next.ids);
        success.jobs.push(...next.jobs);
        end++;
      }

      const details = end === index + 1 ? message.details : { ...message.details, jobs: success.jobs };
      projected.push({
        ...message,
        details,
        content: replaceText(message.content, renderProjection(success, false)),
      });
      changed = true;
      index = end - 1;
      continue;
    }

    const waiting = waitProjection(message, index, origins);

    if (waiting) {
      projected.push({ ...message, content: replaceText(message.content, renderProjection(waiting, true)) });
      changed = true;
    } else {
      projected.push(message);
    }
  }

  return changed ? projected : messages;
}

export function installGuDelivery(pi: any, getOrigins?: (ctx: any) => readonly WorkerTaskOrigin[]): void {
  pi.on("context", (event: any, ctx: any) => {
    if (!Array.isArray(event?.messages)) return undefined;

    const messages = compactGuMessages(event.messages, getOrigins?.(ctx) ?? []);

    return messages === event.messages ? undefined : { messages };
  });
}

function collectOrigins(messages: any[], enrolled: readonly WorkerTaskOrigin[]): Map<string, Origin | null> {
  const origins = new Map<string, Origin | null>();

  for (const row of enrolled) rememberOrigin(origins, row.id, row.batch, row.agent, -1);

  for (const [index, message] of messages.entries()) {
    if (message?.role !== "toolResult" || message.toolName !== "task") continue;
    if (!Array.isArray(message.details?.progress)) continue;

    for (const row of message.details.progress) {
      rememberOrigin(origins, row?.id, message.toolCallId, row?.agent, index);
    }
  }

  return origins;
}

function rememberOrigin(origins: Map<string, Origin | null>, id: unknown, batch: unknown, agent: unknown, index: number): void {
  if (!nonempty(id)) return;

  const previous = origins.get(id);

  if (!nonempty(batch) || !nonempty(agent) || !agent.startsWith("gu-")) {
    origins.set(id, null);
    return;
  }
  if (origins.has(id)) {
    if (!previous || previous.batch !== batch || previous.agent !== agent) origins.set(id, null);
    return;
  }

  origins.set(id, { batch, agent, index });
}

function successProjection(message: any, index: number, origins: Map<string, Origin | null>): Projection | undefined {
  if (message?.role !== "custom" || message.customType !== "async-result") return undefined;
  if (!textOnly(message.content) || message.isError || hasEntries(message.details?.images)) return undefined;

  return projectJobs(message.details?.jobs, index, origins, false);
}

function waitProjection(message: any, index: number, origins: Map<string, Origin | null>): Projection | undefined {
  if (message?.role !== "toolResult" || message.toolName !== "wait") return undefined;
  if (!textOnly(message.content) || message.isError || !record(message.details)) return undefined;
  if (hasEntries(message.details.agents) || hasEntries(message.details.cancelled)) return undefined;
  if (hasEntries(message.details.images) || "errorText" in message.details) return undefined;

  return projectJobs(message.details.jobs, index, origins, true);
}

function projectJobs(jobs: any, index: number, origins: Map<string, Origin | null>, waiting: boolean): Projection | undefined {
  if (!Array.isArray(jobs) || jobs.length === 0) return undefined;

  const projection: Projection = { batch: "", ids: [], running: 0, jobs: [] };
  const seen = new Set<string>();

  for (const job of jobs) {
    if (!record(job) || job.type !== "task" || "errorText" in job || hasEntries(job.images)) return undefined;
    if (job.status !== "completed" && !(waiting && job.status === "running")) return undefined;
    if (!validSchema(job.schema) || !validSchema(job.structured)) return undefined;

    const id = waiting ? job.agentUrlId : job.agentId;
    const origin = nonempty(id) ? origins.get(id) : undefined;

    if (!origin || origin.index >= index || seen.has(id)) return undefined;
    if (projection.batch && projection.batch !== origin.batch) return undefined;

    projection.batch = origin.batch;
    projection.ids.push(id);
    projection.jobs.push(job);
    projection.running += job.status === "running" ? 1 : 0;
    seen.add(id);
  }

  return projection;
}

function renderProjection(projection: Projection, waiting: boolean): string {
  const header = waiting
    ? `Gu tasks: ${projection.running} running, ${projection.ids.length - projection.running} completed.`
    : `Gu tasks: ${projection.ids.length} completed.`;

  return [header, "Read full results before relying on them.", ...projection.ids.map(id => `agent://${id}`)].join("\n");
}

function replaceText(content: any, text: string): any {
  return typeof content === "string" ? text : [{ type: "text", text }];
}

function textOnly(content: any): boolean {
  return typeof content === "string" || (
    Array.isArray(content) && content.length > 0 && content.every(block => record(block) && block.type === "text" && typeof block.text === "string")
  );
}

function validSchema(schema: any): boolean {
  return schema === undefined || (record(schema) && schema.status === "valid" && Object.hasOwn(schema, "data") && schema.error === undefined);
}

function hasEntries(value: any): boolean {
  return value !== undefined && (!Array.isArray(value) || value.length > 0);
}

function nonempty(value: any): value is string {
  return typeof value === "string" && value.trim().length > 0;
}

function record(value: any): value is Record<string, any> {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

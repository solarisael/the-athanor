import { describe, expect, test, afterEach } from "bun:test";

import solarisaelHouseProof from "../index.ts";
type CapturedTool = {
  name: string;
  description?: string;
  parameters: Schema;
  approval?: string;
  execute?: (...args: unknown[]) => Promise<{ content?: Array<{ text: string }>; details?: unknown }>;
};

type Schema = {
  kind: "string" | "boolean" | "number" | "enum" | "literal" | "object" | "array" | "discriminatedUnion";
  description?: string;
  isOptional?: boolean;
  pattern?: string;
  values?: string[];
  shape?: Record<string, Schema>;
  element?: Schema;
  variants?: Schema[];
  describe(description: string): Schema;
  regex(pattern: RegExp): Schema;
  optional(): Schema;
  default(value: unknown): Schema;
};

type SchemaSummary =
  | { type: "string"; optional?: true; pattern?: string }
  | { type: "boolean"; optional?: true }
  | { type: "number"; optional?: true }
  | { type: "enum"; values: string[]; optional?: true }
  | { type: "literal"; value: string | boolean; optional?: true }
  | { type: "array"; element: SchemaSummary; optional?: true }
  | { type: "object"; fields: Record<string, SchemaSummary>; optional?: true }
  | { type: "discriminatedUnion"; variants: SchemaSummary[]; optional?: true };

function makeSchema(kind: Schema["kind"], fields: Partial<Schema> = {}): Schema {
  return {
    kind,
    ...fields,
    describe(description: string) {
      this.description = description;
      return this;
    },
    regex(pattern: RegExp) {
      this.pattern = pattern.source;
      return this;
    },
    optional() {
      this.isOptional = true;
      return this;
    },
    default(_value: unknown) {
      return this;
    },
  } as Schema;
}

const zodStub = {
  string() {
    return makeSchema("string");
  },
  boolean() {
    return makeSchema("boolean");
  },
  enum(values: string[]) {
    return makeSchema("enum", { values });
  },
  literal(value: string | boolean) {
    return makeSchema("literal", { values: [String(value)] });
  },
  discriminatedUnion(_key: string, variants: Schema[]) {
    return makeSchema("discriminatedUnion", { variants });
  },
  object(shape: Record<string, Schema>) {
    return makeSchema("object", { shape });
  },
  number() {
    return makeSchema("number");
  },
  array(element: Schema) {
    return makeSchema("array", { element });
  },
};

function summarizeSchema(schema: Schema): SchemaSummary {
  const optional = schema.isOptional ? { optional: true as const } : {};

  switch (schema.kind) {
    case "string":
      return { type: "string", ...(schema.pattern ? { pattern: schema.pattern } : {}), ...optional };
    case "boolean":
      return { type: "boolean", ...optional };
    case "number":
      return { type: "number", ...optional };
    case "enum":
      return { type: "enum", values: schema.values ?? [], ...optional };
    case "literal": {
      const value = schema.values?.[0] ?? "";
      return { type: "literal", value: value === "true" ? true : value === "false" ? false : value, ...optional };
    }
    case "array":
      if (!schema.element) throw new Error("array schema missing element");
      return { type: "array", element: summarizeSchema(schema.element), ...optional };
    case "discriminatedUnion":
      return { type: "discriminatedUnion", variants: (schema.variants ?? []).map(summarizeSchema), ...optional };
    case "object":
      return {
        type: "object",
        fields: Object.fromEntries(
          Object.entries(schema.shape ?? {}).map(([key, value]) => [key, summarizeSchema(value)]),
        ),
        ...optional,
      };
  }
}

function registerAdapter() {
  const labels: string[] = [];
  const hooks: Array<{ name: string; handler: unknown }> = [];
  const eventChannels: string[] = [];
  const messageRenderers: Array<{ customType: string; renderer: unknown }> = [];
  const tools: CapturedTool[] = [];

  const pi = {
    zod: zodStub,
    setLabel(label: string) {
      labels.push(label);
    },
    on(name: string, handler: unknown) {
      hooks.push({ name, handler });
    },
    events: {
      on(channel: string, _handler: unknown) {
        eventChannels.push(channel);
        return () => {};
      },
    },
    registerMessageRenderer(customType: string, renderer: unknown) {
      messageRenderers.push({ customType, renderer });
    },
    registerTool(tool: CapturedTool) {
      tools.push(tool);
    },
  };
  solarisaelHouseProof(pi);
  return { labels, hooks, tools, eventChannels, messageRenderers };
}

const expectedToolNames = [
  "recall",
  "canon_read",
  "canon_write",
  "remember",
  "delete_lesson",
  "update_lesson",
  "wake",
  "anamnesis",
  "anamnesis_write",
  "room_state",
  "set_room_state",
  "lessons",
  "design_doc",
  "design_doc_write",
  "sleep",
  "hallway_create",
  "hallway_join",
  "hallway_post",
  "hallway_knock_policy",
  "hallway_knock",
  "hallway_read",
  "hallway_inbox",
  "house_lane_status",
  "familiar_status",
  "familiar_dispatch",
  "house_dispatch",
  "house_routing_mode",
  "kitten_lineage_status",
  "recall_policy",
  "house_model_default",
  "giga_candidate_list",
  "giga_health",
  "giga_queue_maintenance",
  "giga_review",
  "giga_promote_memory",
  "giga_promote_coding_lesson",
  "giga_promote_project_lesson",
];

function toolMap(tools: CapturedTool[]) {
  return Object.fromEntries(tools.map((tool) => [tool.name, tool]));
}

describe("OMP adapter registration", () => {
  test("registers the public adapter label and lifecycle hooks", () => {
    const { labels, hooks, eventChannels, messageRenderers } = registerAdapter();

    expect(labels).toEqual(["The Athanor"]);
    expect(hooks.map((hook) => hook.name)).toEqual(["session_start", "session_switch", "session_shutdown", "tool_call", "tool_result", "tool_call", "tool_call", "message_start", "message_start", "message_update", "context", "session_compact", "tool_result", "tool_result", "shutdown", "agent_end"]);
    expect(hooks.every((hook) => typeof hook.handler === "function")).toBe(true);
    expect(eventChannels).toEqual(["task:subagent:progress", "task:subagent:lifecycle"]);
    expect(messageRenderers).toHaveLength(2);
    expect(messageRenderers.map((entry) => entry.customType)).toEqual([
      "solarisael-lesson-trigger",
      "solarisael-process-lessons",
    ]);
    expect(messageRenderers.every((entry) => typeof entry.renderer === "function")).toBe(true);
  });

  test("observes the tool lifecycle without answering for the tools it watches", async () => {
    const { hooks } = registerAdapter();
    const observers = [hooks[3], hooks[4]];

    expect(observers.map((hook) => hook.name)).toEqual(["tool_call", "tool_result"]);
    for (const observer of observers) {
      const handler = observer.handler as (event: unknown, ctx: unknown) => Promise<unknown>;
      // A malformed event, a missing ctx, and a tool result with no observed
      // call all resolve to nothing: an Insula tap never blocks, rewrites, or
      // refuses the turn it is watching.
      expect(await handler({ toolCallId: "call-1", isError: true }, {})).toBeUndefined();
      expect(await handler({}, undefined)).toBeUndefined();
      expect(await handler(undefined, undefined)).toBeUndefined();
    }
  });

  test("keeps reused tool ids session-local and cancels a closing session", async () => {
    const original = {
      disabled: process.env.ATHANOR_DISABLE_INSULA,
      endpoints: process.env.ATHANOR_HOST_ENDPOINTS,
      socket: process.env.ATHANOR_HOST_WS_URL,
      token: process.env.ATHANOR_HOST_TOKEN,
    };
    const received: any[] = [];
    const server = Bun.serve({
      port: 0,
      hostname: "127.0.0.1",
      async fetch(request) {
        const body = await request.json() as { events?: any[] };
        received.push(...(body.events ?? []));
        return Response.json({
          schemaVersion: 1,
          acceptedCount: body.events?.length ?? 0,
          duplicateCount: 0,
          conflicts: [],
        });
      },
    });
    try {
      process.env.ATHANOR_DISABLE_INSULA = "0";
      process.env.ATHANOR_HOST_ENDPOINTS = JSON.stringify({
        "default-room": { url: `ws://127.0.0.1:${server.port}/athanor/v1/ws` },
      });
      delete process.env.ATHANOR_HOST_WS_URL;
      process.env.ATHANOR_HOST_TOKEN = "adapter-registration-insula-token";

      const { hooks } = registerAdapter();
      const toolCall = hooks[3]!.handler as (event: unknown, ctx: unknown) => Promise<unknown>;
      const toolResult = hooks[4]!.handler as (event: unknown, ctx: unknown) => Promise<unknown>;
      const sessionShutdown = hooks[2]!.handler as (event: unknown, ctx: unknown) => Promise<unknown>;
      const shutdown = hooks.find((hook) => hook.name === "shutdown")!.handler as () => Promise<unknown>;
      const firstSession = { cwd: process.cwd(), sessionId: "session-a" };
      const secondSession = { cwd: process.cwd(), sessionId: "session-b" };

      await toolCall({ toolCallId: "reused-call-id" }, firstSession);
      await toolCall({ toolCallId: "reused-call-id" }, secondSession);
      await sessionShutdown({}, firstSession);
      await toolResult({ toolCallId: "reused-call-id", isError: false }, secondSession);
      await shutdown();

      const tools = received.filter((event) => event.operation === "tool_call");
      expect(tools.map((event) => `${event.phase}:${event.outcomeClass}`)).toEqual([
        "start:unknown",
        "start:unknown",
        "end:cancelled",
        "end:ok",
      ]);
      expect(new Set(tools.filter((event) => event.phase === "start").map((event) => event.spanId)).size)
        .toBe(2);
      for (const start of tools.filter((event) => event.phase === "start")) {
        const end = tools.find((event) => event.phase === "end" && event.spanId === start.spanId);
        expect(end).toBeDefined();
      }
    } finally {
      server.stop(true);
      if (original.disabled === undefined) delete process.env.ATHANOR_DISABLE_INSULA;
      else process.env.ATHANOR_DISABLE_INSULA = original.disabled;
      if (original.endpoints === undefined) delete process.env.ATHANOR_HOST_ENDPOINTS;
      else process.env.ATHANOR_HOST_ENDPOINTS = original.endpoints;
      if (original.socket === undefined) delete process.env.ATHANOR_HOST_WS_URL;
      else process.env.ATHANOR_HOST_WS_URL = original.socket;
      if (original.token === undefined) delete process.env.ATHANOR_HOST_TOKEN;
      else process.env.ATHANOR_HOST_TOKEN = original.token;
    }
  });

  test("registers the Solarisael tool surface", () => {
    const { tools } = registerAdapter();

    expect(tools).toHaveLength(expectedToolNames.length);

    expect(new Set(tools.map((tool) => tool.name))).toEqual(new Set(expectedToolNames));
    expect(toolMap(tools)).toMatchObject({
      recall: { approval: "read" },
      canon_read: { approval: "read" },
      canon_write: { approval: "write" },
      remember: { approval: "write" },
      delete_lesson: { approval: "write" },
      update_lesson: { approval: "write" },
      wake: { approval: "read" },
      anamnesis: { approval: "read" },
      anamnesis_write: { approval: "write" },
      room_state: { approval: "read" },
      set_room_state: { approval: "write" },
      lessons: { approval: "read" },
      design_doc: { approval: "read" },
      design_doc_write: { approval: "write" },
      sleep: { approval: "write" },
      house_lane_status: { approval: "read" },
      familiar_status: { approval: "read" },
      familiar_dispatch: { approval: "read" },
      house_dispatch: { approval: "read" },
      house_routing_mode: { approval: "write" },
      kitten_lineage_status: { approval: "read" },
      recall_policy: { approval: "write" },
      house_model_default: { approval: "write" },
      giga_promote_memory: { approval: "write" },
      giga_promote_coding_lesson: { approval: "write" },
      giga_promote_project_lesson: { approval: "write" },
    });
  });


  test("teaches the humane continuity register at every memory boundary", () => {
    const tools = toolMap(registerAdapter().tools);

    expect(tools.recall.description).toContain("recognition across time, not dossier lookup");
    expect(tools.recall.parameters.shape?.query.description).toContain("room's own vocabulary");
    expect(tools.remember.description).toContain("care for a future self, not filing a case report");
    expect(tools.remember.parameters.shape?.body.description).toContain("active room's natural voice");
    expect(tools.wake.description).toContain("letter from the room's previous waking self");
    expect(tools.sleep.description).toContain("embodied continuity across sleep");
    expect(tools.sleep.parameters.shape?.body.description).toContain("room's relationship register");
  });
 


  test("exposes the OMP parameter schemas for each registered tool", () => {
    const { tools } = registerAdapter();
    const schemas = Object.fromEntries(
      Object.entries(toolMap(tools)).map(([name, tool]) => [name, summarizeSchema(tool.parameters)]),
    );

    expect(schemas).toEqual({
      recall: {
        type: "object",
        fields: {
          query: { type: "string" },
        },
      },
      canon_read: {
        type: "object",
        fields: {
          id: { type: "string", pattern: "^[1-9]\\d*$", optional: true },
          name: { type: "string", optional: true },
          includeHistory: { type: "boolean", optional: true },
          room: { type: "enum", values: ["house"], optional: true },
        },
      },
      canon_write: {
        type: "object",
        fields: {
          name: { type: "string" },
          kind: { type: "string" },
          summary: { type: "string" },
          aliases: { type: "array", element: { type: "string" }, optional: true },
          searchBoost: { type: "string", optional: true },
          weighty: { type: "boolean", optional: true },
          pointerFiles: {
            type: "array",
            element: {
              type: "object",
              fields: {
                file: { type: "string" },
                lines: { type: "array", element: { type: "number" }, optional: true },
              },
            },
            optional: true,
          },
          summaryAsOf: { type: "string", pattern: "^\\d{4}-\\d{2}-\\d{2}$", optional: true },
          supersedes: {
            type: "array",
            element: { type: "string", pattern: "^[1-9]\\d*$" },
            optional: true,
          },
          room: { type: "enum", values: ["house"], optional: true },
        },
      },
      remember: {
        type: "object",
        fields: {
          title: { type: "string" },
          body: { type: "string" },
          kind: {
            type: "enum",
            values: ["memory", "coding-lesson", "project-lesson", "writing-lesson", "design-lesson", "audio-lesson"],
            optional: true,
          },
          room: { type: "enum", values: ["house"], optional: true },
          threads: { type: "array", element: { type: "string" }, optional: true },
          supersedes: { type: "array", element: { type: "string" }, optional: true },
          continues: {
            type: "array",
            element: {
              type: "object",
              fields: {
                thread: { type: "string" },
                previousMemoryId: { type: "string", pattern: "^[1-9]\\d*$" },
              },
            },
            optional: true,
          },
          shape: { type: "string", optional: true },
          voice: { type: "string", optional: true },
          register: { type: "array", element: { type: "string" }, optional: true },
          scope: { type: "string", optional: true },
          project: { type: "string", optional: true },
          proofPattern: { type: "string", optional: true },
          triggerContext: { type: "string", optional: true },
          exampleText: { type: "string", optional: true },
          languageKeys: { type: "array", element: { type: "string" }, optional: true },
          technologyKeys: { type: "array", element: { type: "string" }, optional: true },
          threadKeys: { type: "array", element: { type: "string" }, optional: true },
          tags: { type: "array", element: { type: "string" }, optional: true },
          sourceMemoryPath: { type: "string", optional: true },
          condition: { type: "array", element: { type: "string" }, optional: true },
          astCondition: { type: "array", element: { type: "string" }, optional: true },
          triggerScope: { type: "array", element: { type: "string" }, optional: true },
          interruptMode: { type: "enum", values: ["block", "remind"], optional: true },
          repeatCooldownSecs: { type: "number", optional: true },
        },
      },
      delete_lesson: {
        type: "object",
        fields: {
          kind: { type: "enum", values: ["coding-lesson", "project-lesson", "writing-lesson", "design-lesson"] },
          id: { type: "string" },
          expectedTitle: { type: "string" },
        },
      },
      update_lesson: {
        type: "object",
        fields: {
          kind: { type: "enum", values: ["coding-lesson", "project-lesson", "writing-lesson", "design-lesson"] },
          id: { type: "string" },
          expectedTitle: { type: "string" },
          patch: {
            type: "object",
            fields: {
              title: { type: "string", optional: true },
              body: { type: "string", optional: true },
              shape: { type: "string", optional: true },
              triggerContext: { type: "string", optional: true },
              tags: { type: "array", element: { type: "string" }, optional: true },
              voice: { type: "string", optional: true },
              scope: { type: "string", optional: true },
              alwaysOn: { type: "boolean", optional: true },
              project: { type: "string", optional: true },
              clearProject: { type: "boolean", optional: true },
              proofPattern: { type: "string", optional: true },
              languageKeys: { type: "array", element: { type: "string" }, optional: true },
              technologyKeys: { type: "array", element: { type: "string" }, optional: true },
              threadKeys: { type: "array", element: { type: "string" }, optional: true },
              register: { type: "array", element: { type: "string" }, optional: true },
              exampleText: { type: "string", optional: true },
              writers: { type: "array", element: { type: "string" }, optional: true },
              negationOf: { type: "string", optional: true },
              condition: { type: "array", element: { type: "string" }, optional: true },
              astCondition: { type: "array", element: { type: "string" }, optional: true },
              triggerScope: { type: "array", element: { type: "string" }, optional: true },
              interruptMode: { type: "enum", values: ["block", "remind"], optional: true },
              repeatCooldownSecs: { type: "number", optional: true },
            },
          },
        },
      },
      wake: { type: "object", fields: {} },
      anamnesis: {
        type: "object",
        fields: {
          mode: { type: "enum", values: ["wake", "consult"] },
          query: { type: "string", optional: true },
          limit: { type: "number", optional: true },
        },
      },
      anamnesis_write: {
        type: "object",
        fields: {
          operation: { type: "enum", values: ["add", "append-rep"] },
          kind: { type: "enum", values: ["pillar", "cycle"], optional: true },
          fidelity: { type: "enum", values: ["record", "raw-material"], optional: true },
          activation: { type: "enum", values: ["wake", "fork"], optional: true },
          dormant: { type: "boolean", optional: true },
          title: { type: "string" },
          shape: { type: "string", optional: true },
          ramp: { type: "string", optional: true },
          counsel: { type: "string", optional: true },
          peak: { type: "string", optional: true },
          beginning: { type: "string", optional: true },
          verifyNote: { type: "string", optional: true },
          canon: { type: "array", element: { type: "string" }, optional: true },
          sourcePaths: { type: "array", element: { type: "string" }, optional: true },
          tags: { type: "array", element: { type: "string" }, optional: true },
          allowEmptyCycle: { type: "boolean", optional: true },
          seedRep: { type: "object", optional: true, fields: { number: { type: "number" }, occurredOn: { type: "string", optional: true }, howItWent: { type: "string" }, portalPull: { type: "string" }, lighter: { type: "string" } } },
          repNumber: { type: "number", optional: true },
          occurredOn: { type: "string", optional: true },
          howItWent: { type: "string", optional: true },
          portalPull: { type: "string", optional: true },
          lighter: { type: "string", optional: true },
        },
      },
      giga_candidate_list: {
        type: "object",
        fields: {
          review_state: {
            type: "enum",
            values: ["unreviewed", "in_review", "dismissed", "unresolved", "curio", "expired"],
            optional: true,
          },
          limit: { type: "number", optional: true },
        },
      },
      giga_health: { type: "object", fields: {} },
      giga_queue_maintenance: {
        type: "object",
        fields: {
          operation: { type: "enum", values: ["check", "purge_stuck"] },
        },
      },
      giga_review: {
        type: "object",
        fields: {
          candidate_id: { type: "string" },
          new_state: {
            type: "enum",
            values: ["in_review", "dismissed", "unresolved", "curio", "expired"],
          },
          reason: { type: "string" },
        },
      },
      giga_promote_memory: {
        type: "object",
        fields: {
          candidate_id: { type: "string" },
          title: { type: "string" },
          body: { type: "string" },
          threads: { type: "array", element: { type: "string" }, optional: true },
        },
      },
      giga_promote_coding_lesson: {
        type: "object",
        fields: {
          candidate_id: { type: "string" },
          title: { type: "string" },
          body: { type: "string" },
          shape: { type: "string", optional: true },
          proof_pattern: { type: "string", optional: true },
          trigger_context: { type: "string", optional: true },
          language_keys: { type: "array", element: { type: "string" }, optional: true },
          technology_keys: { type: "array", element: { type: "string" }, optional: true },
          tags: { type: "array", element: { type: "string" }, optional: true },
        },
      },
      giga_promote_project_lesson: {
        type: "object",
        fields: {
          candidate_id: { type: "string" },
          title: { type: "string" },
          body: { type: "string" },
          proof_pattern: { type: "string", optional: true },
          trigger_context: { type: "string", optional: true },
          language_keys: { type: "array", element: { type: "string" }, optional: true },
          technology_keys: { type: "array", element: { type: "string" }, optional: true },
          tags: { type: "array", element: { type: "string" }, optional: true },
          publication_approved: { type: "boolean" },
        },
      },
      room_state: { type: "object", fields: {} },
      kitten_lineage_status: { type: "object", fields: {} },
      recall_policy: {
        type: "object",
        fields: {
          requestedMode: {
            type: "enum",
            values: ["auto", "conversation", "work", "quiet"],
            optional: true,
          },
        },
      },
      set_room_state: {
        type: "object",
        fields: {
          operator: { type: "string", optional: true },
          embodiedSpirit: { type: "string", optional: true },
        },
      },
      lessons: {
        type: "object",
        fields: {
          type: { type: "enum", values: ["coding", "project", "writing", "design", "audio"] },
          shape: { type: "string", optional: true },
          project: { type: "string", optional: true },
          register: { type: "string", optional: true },
          stage: { type: "string", optional: true },
          languageKeys: { type: "array", element: { type: "string" }, optional: true },
          technologyKeys: { type: "array", element: { type: "string" }, optional: true },
          query: { type: "string", optional: true },
          limit: { type: "number" },
        },
      },
      design_doc: {
        type: "object",
        fields: {
          system: { type: "string" },
          docType: { type: "enum", values: ["token", "component", "contract", "guideline"], optional: true },
          name: { type: "string", optional: true },
          group: { type: "string", optional: true },
          query: { type: "string", optional: true },
          includeSuperseded: { type: "boolean", optional: true },
          limit: { type: "number" },
        },
      },
      design_doc_write: {
        type: "object",
        fields: {
          system: { type: "string" },
          docType: { type: "enum", values: ["token", "component", "contract", "guideline"] },
          name: { type: "string" },
          group: { type: "string", optional: true },
          values: { type: "object", optional: true, fields: {} },
          body: { type: "string", optional: true },
          provenance: { type: "object", optional: true, fields: {} },
          tags: { type: "array", element: { type: "string" }, optional: true },
          supersedes: { type: "string", optional: true },
          allowIdentityChange: { type: "boolean", optional: true },
        },
      },
      sleep: {
        type: "object",
        fields: {
          body: { type: "string" },
        },
      },
      hallway_create: {
        type: "object",
        fields: {
          hallway: { type: "string" },
          allowed_rooms: { type: "array", element: { type: "string" } },
          idempotency_key: { type: "string", optional: true },
        },
      },
      hallway_join: {
        type: "object",
        fields: {
          hallway: { type: "string" },
          idempotency_key: { type: "string", optional: true },
        },
      },
      hallway_post: {
        type: "object",
        fields: {
          hallway: { type: "string" },
          body: { type: "string" },
          reply_to: { type: "number", optional: true },
          to_rooms: { type: "array", element: { type: "string" }, optional: true },
          idempotency_key: { type: "string", optional: true },
        },
      },
      hallway_knock_policy: {
        type: "object",
        fields: {
          hallway: { type: "string" },
          mode: { type: "enum", values: ["manual", "allow_list"] },
          allowed_rooms: { type: "array", element: { type: "string" }, optional: true },
          max_turns: { type: "number", optional: true },
          idempotency_key: { type: "string", optional: true },
        },
      },
      hallway_knock: {
        type: "object",
        fields: {
          hallway: { type: "string" },
          message_id: { type: "number" },
          recipient_room: { type: "string" },
          parent_knock_id: { type: "string", optional: true },
          max_turns: { type: "number", optional: true },
          idempotency_key: { type: "string", optional: true },
        },
      },
      hallway_read: {
        type: "object",
        fields: {
          hallway: { type: "string" },
          after: { type: "number", optional: true },
          thread: { type: "string", optional: true },
          limit: { type: "number", optional: true },
          advance_cursor: { type: "boolean", optional: true },
        },
      },
      hallway_inbox: { type: "object", fields: {} },
      house_lane_status: { type: "object", fields: {} },
      familiar_status: { type: "object", fields: {} },
      familiar_dispatch: {
        type: "object",
        fields: {
          familiar: { type: "string" },
          task: { type: "string" },
          target: { type: "string", optional: true },
          context: {
            type: "array",
            optional: true,
            element: {
              type: "object",
              fields: {
                mode: { type: "enum", values: ["exact", "gist", "image-ok", "retrieve-only"] },
                source: { type: "string", optional: true },
                content: { type: "string", optional: true },
                reason: { type: "string", optional: true },
              },
            },
          },
          acceptance: { type: "array", element: { type: "string" }, optional: true },
          lessonBodies: { type: "array", element: { type: "string" }, optional: true },
          risk: { type: "enum", values: ["low", "medium", "high"], optional: true },
        },
      },
      house_dispatch: {
        type: "object",
        fields: {
          lane: { type: "string", optional: true },
          familiar: { type: "string", optional: true },
          task: { type: "string" },
          target: { type: "string", optional: true },
          context: {
            type: "array",
            optional: true,
            element: {
              type: "object",
              fields: {
                mode: { type: "enum", values: ["exact", "gist", "image-ok", "retrieve-only"] },
                source: { type: "string", optional: true },
                content: { type: "string", optional: true },
                reason: { type: "string", optional: true },
              },
            },
          },
          acceptance: { type: "array", element: { type: "string" }, optional: true },
          lessonBodies: { type: "array", element: { type: "string" }, optional: true },
          risk: { type: "enum", values: ["low", "medium", "high"], optional: true },
        },
      },
      house_routing_mode: {
        type: "object",
        fields: {
          enabled: { type: "boolean", optional: true },
        },
      },
      house_model_default: {
        type: "object",
        fields: {
          model: { type: "string", optional: true },
          enabled: { type: "boolean", optional: true },
          applyNow: { type: "boolean", optional: true },
          clear: { type: "boolean", optional: true },
        },
      },
    });
  });

});

import { describe, expect, test } from "bun:test";
import { mkdtemp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import {
  loadMemoryContentSource,
  loadMemoryDateSource,
  loadMemoryLexicalSources,
  loadMemorySemanticSource,
} from "../src/memory-sources.ts";
import { injectRoomMemoryContext, runRecallQuery } from "../src/memory.ts";

const ENV_KEYS = [
  "SOLARISAEL_MEMORY_SOURCE",
  "SOLARISAEL_HOUSE_DISABLE_POSTGRES",
  "KINTSU_MEM_DEBUG",
];

function snapshotEnv() {
  return Object.fromEntries(ENV_KEYS.map((key) => [key, process.env[key]]));
}

function restoreEnv(snapshot) {
  for (const key of ENV_KEYS) {
    const value = snapshot[key];
    if (value === undefined) {
      delete process.env[key];
    } else {
      process.env[key] = value;
    }
  }
}

async function withForcedJsonEnv(fn) {
  const snapshot = snapshotEnv();
  try {
    process.env.SOLARISAEL_MEMORY_SOURCE = "json";
    delete process.env.SOLARISAEL_HOUSE_DISABLE_POSTGRES;
    return await fn();
  } finally {
    restoreEnv(snapshot);
  }
}


async function writeJson(target, value) {
  await mkdir(path.dirname(target), { recursive: true });
  await writeFile(target, `${JSON.stringify(value, null, 2)}\n`, "utf8");
}

async function createKintsuFixture() {
  const root = await mkdtemp(path.join(tmpdir(), "solarisael-memory-json-"));
  const roomDir = path.join(root, "kintsu");
  await mkdir(path.join(roomDir, "memory"), { recursive: true });

  await writeFile(
    path.join(roomDir, "memory", "2026-07-04_blue_hinge.md"),
    [
      "# Blue hinge protocol",
      "The blue hinge keeps lantern retrieval deterministic without Postgres.",
      "It references the hidden anchor canon so the active thread surfaces a canon assertion.",
      "Use the blue hinge when checking JSON-only memory.",
    ].join("\n"),
    "utf8",
  );
  await writeFile(
    path.join(roomDir, "memory", "2026-07-04_second_lantern.md"),
    [
      "# Second lantern entry",
      "The second lantern shares the same blue hinge thread.",
      "It proves recall returns one JSON candidate per ranked thread entry.",
    ].join("\n"),
    "utf8",
  );

  const index = {
    files: {
      "memory/2026-07-04_blue_hinge.md": {
        one_line: "Blue hinge protocol keeps lantern retrieval deterministic.",
      },
      "memory/2026-07-04_second_lantern.md": {
        one_line: "Second lantern entry proves multi-entry JSON recall candidates.",
      },
    },
    threads: {
      "blue hinge / lantern protocol": [
        {
          file: "memory/2026-07-04_blue_hinge.md",
          lines: [1, 4],
          context: "blue hinge lantern retrieval forced json",
        },
        {
          file: "memory/2026-07-04_second_lantern.md",
          lines: [1, 3],
          context: "blue hinge lantern recall second entry",
        },
      ],
    },
  };

  const importantIndex = {
    "hidden anchor canon": {
      type: "project",
      summary: "The hidden anchor canon must surface when its pointer file is active.",
      aliases: ["quiet anchor"],
      files: [
        {
          file: "memory/2026-07-04_blue_hinge.md",
          lines: [1, 4],
        },
      ],
    },
    "named lantern canon": {
      type: "memory",
      summary: "Named lantern canon is returned by direct recall term matching.",
      aliases: ["lantern canon"],
      files: [
        {
          file: "memory/2026-07-04_second_lantern.md",
          lines: [1, 3],
        },
      ],
    },
  };

  await writeJson(path.join(roomDir, "memory", "index.json"), index);
  await writeJson(path.join(roomDir, "memory", "important_index.json"), { entries: importantIndex });

  return { root, roomDir, index, importantIndex };
}

describe("forced JSON memory sources", () => {
  test("lexical source returns JSON indexes with the forced JSON fallback reason", async () => {
    const { root, roomDir, index, importantIndex } = await createKintsuFixture();
    try {
      await withForcedJsonEnv(async () => {
        const result = await loadMemoryLexicalSources(roomDir, "kintsu", "blue hinge lantern");

        expect(result.indexSource).toBe("json");
        expect(result.importantSource).toBe("json");
        expect(result.fallbackReason).toBe("forced json memory source");
        expect(result.index.files).toEqual(index.files);
        expect(result.index.threads).toEqual(index.threads);
        expect(result.importantIndex).toEqual(importantIndex);
      });
    } finally {
      await rm(root, { recursive: true, force: true });
    }
  });

  test("semantic, content, and date loaders preserve preflight skip labels and force-skip live sources", async () => {
    const { root, roomDir } = await createKintsuFixture();
    try {
      await withForcedJsonEnv(async () => {
        await expect(loadMemorySemanticSource(roomDir, "kintsu", "", ["memory/a.md"]))
          .resolves.toMatchObject({ semanticSource: "skip-no-scope", semanticChunks: [] });
        await expect(loadMemorySemanticSource(roomDir, "kintsu", "blue hinge", []))
          .resolves.toMatchObject({ semanticSource: "skip-no-scope", semanticChunks: [] });
        await expect(loadMemorySemanticSource(roomDir, "kintsu", "blue hinge", ["memory/2026-07-04_blue_hinge.md"]))
          .resolves.toMatchObject({ semanticSource: "skip-forced-json", semanticChunks: [] });

        await expect(loadMemoryContentSource(roomDir, "kintsu", ""))
          .resolves.toMatchObject({ contentSource: "skip-no-prompt", contentChunks: [] });
        await expect(loadMemoryContentSource(roomDir, "kintsu", "blue hinge"))
          .resolves.toMatchObject({ contentSource: "skip-forced-json", contentChunks: [] });

        await expect(loadMemoryDateSource(roomDir, "kintsu", ""))
          .resolves.toMatchObject({ dateSource: "skip-no-prompt", dateMatches: [], queryDates: [] });
        await expect(loadMemoryDateSource(roomDir, "kintsu", "blue hinge 2026-07-04"))
          .resolves.toMatchObject({ dateSource: "skip-forced-json", dateMatches: [], queryDates: [] });
      });
    } finally {
      await rm(root, { recursive: true, force: true });
    }
  });
});

describe("forced JSON memory orchestration", () => {
  test("injectRoomMemoryContext adds memory and canon system-reminder parts from JSON fixtures", async () => {
    const { root, roomDir } = await createKintsuFixture();
    try {
      await withForcedJsonEnv(async () => {
        const output = {
          messages: [
            {
              info: { id: "msg-1", role: "user", sessionID: "json-memory-smoke" },
              parts: [
                {
                  id: "part-1",
                  sessionID: "json-memory-smoke",
                  messageID: "msg-1",
                  type: "text",
                  text: "Please use the blue hinge lantern protocol.",
                },
              ],
            },
          ],
        };

        await injectRoomMemoryContext(output, { roomDir });

        const userParts = output.messages[0].parts;
        const memoryPart = userParts.find((part) => part.metadata?.solarisaelMemory === true);
        const canonPart = userParts.find((part) => part.metadata?.solarisaelCanonAssertion === true);

        expect(memoryPart?.synthetic).toBe(true);
        expect(memoryPart?.text).toContain("Room memory was retrieved automatically for this user turn.");
        expect(memoryPart?.text).toContain("## Kintsu Memory Retrieval - auto-loaded context");
        expect(memoryPart?.text).toContain("memory/2026-07-04_blue_hinge.md:1-4");
        expect(memoryPart?.text).toContain("The blue hinge keeps lantern retrieval deterministic without Postgres.");

        expect(canonPart?.synthetic).toBe(true);
        expect(canonPart?.text).toContain("Canon assertions for this turn");
        expect(canonPart?.text).toContain("## Kintsu Canon Assertions");
        expect(canonPart?.text).toContain("### hidden anchor canon (project)");
        expect(canonPart?.text).toContain("The hidden anchor canon must surface when its pointer file is active.");
      });
    } finally {
      await rm(root, { recursive: true, force: true });
    }
  });

  test("casual route records semantic and content as skipped by query routing without live sources", async () => {
    const { root, roomDir } = await createKintsuFixture();
    try {
      await withForcedJsonEnv(async () => {
        process.env.KINTSU_MEM_DEBUG = "1";
        const output = {
          messages: [
            {
              info: { id: "msg-casual", role: "user", sessionID: "json-memory-casual-route" },
              parts: [
                {
                  id: "part-casual",
                  sessionID: "json-memory-casual-route",
                  messageID: "msg-casual",
                  type: "text",
                  text: "hello love",
                },
              ],
            },
          ],
        };

        await injectRoomMemoryContext(output, { roomDir });

        const debugLog = await readFile(path.join(roomDir, "memory", "retrieval_debug.log"), "utf8");
        expect(debugLog).toContain("semantic_source=skip-query-route");
        expect(debugLog).toContain("content_source=skip-query-route");
      });
    } finally {
      await rm(root, { recursive: true, force: true });
    }
  });

  test("runRecallQuery returns JSON candidates for every entry in a ranked thread plus canon matches", async () => {
    const { root, roomDir } = await createKintsuFixture();
    try {
      await withForcedJsonEnv(async () => {
        const result = await runRecallQuery(roomDir, "kintsu", "blue hinge lantern canon");

        expect(result.ok).toBe(true);
        expect(result.semanticChunks).toEqual([]);
        expect(result.contentChunks).toEqual([]);
        expect(result.dateMatches).toEqual([]);
        expect(result.found).toBe(true);

        const searchCandidatePaths = result.searchCandidates.map((candidate) => candidate.source_path);
        expect(searchCandidatePaths).toContain("memory/2026-07-04_blue_hinge.md");
        expect(searchCandidatePaths).toContain("memory/2026-07-04_second_lantern.md");

        const retrievalCandidatePaths = result.retrievalCandidates.map((candidate) => candidate.source_path);
        expect(retrievalCandidatePaths).toContain("memory/2026-07-04_blue_hinge.md");
        expect(retrievalCandidatePaths).toContain("memory/2026-07-04_second_lantern.md");

        expect(result.canonMatches.map((match) => match.termKey)).toEqual(
          expect.arrayContaining(["named lantern canon", "hidden anchor canon"]),
        );
      });
    } finally {
      await rm(root, { recursive: true, force: true });
    }
  });
});

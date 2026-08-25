import { runLessonQuery } from "./lesson-context.ts";
import type { WorkContext } from "./recall-policy.ts";

export type PacketLesson = { family: string; id: number; title: string; body: string; proofPattern: string };

// 24 by Sol's word (2026-08-25): 12 starved design once the family doors opened.
const PACKET_BUDGET = 24;
// enough: keyed rows compete inside a 50-row recency window because the wire
// has no keyed-only filter; a keyedOnly flag on lesson_query is the way up.
const KEYED_FETCH_LIMIT = 50;

function normalizeLesson(value: unknown): { lesson: PacketLesson; keyed: boolean } | null {
  if (!value || typeof value !== "object" || Array.isArray(value)) return null;
  const row = value as Record<string, unknown>;
  const family = String(row.type ?? "").trim();
  const id = Number(row.id);
  const title = String(row.title ?? "").trim();
  const body = String(row.lesson ?? "").trim();
  const proofPattern = String(row.proofPattern ?? "").trim();
  if (!family || !Number.isInteger(id) || id <= 0 || !title || !body) return null;
  const keyed = (Array.isArray(row.languageKeys) && row.languageKeys.length > 0)
    || (Array.isArray(row.technologyKeys) && row.technologyKeys.length > 0);
  return { lesson: { family, id, title, body, proofPattern }, keyed };
}

function queryRows(query: Record<string, unknown>): Array<{ lesson: PacketLesson; keyed: boolean }> | null {
  if (!query || query.ok !== true || !Array.isArray(query.lessons)) return null;
  return query.lessons.flatMap((value) => {
    const entry = normalizeLesson(value);
    return entry ? [entry] : [];
  });
}

// Per family: always-on rows union evidence-keyed rows. The union exists
// because the eligibility filter treats empty keys as unkeyed-only — the exact
// hole that kept keyed lessons out of every packet before 2026-08-24.
async function familyLessons(
  roomDir: string,
  room: string,
  family: string,
  keys: Record<string, unknown>,
  hasKeys: boolean,
  warnings: string[],
): Promise<PacketLesson[]> {
  const rows: PacketLesson[] = [];
  const seen = new Set<number>();
  const alwaysOn = queryRows(
    await runLessonQuery(roomDir, room, { type: family, alwaysOn: true, ...keys, limit: PACKET_BUDGET }) as Record<string, unknown>,
  );
  if (alwaysOn) {
    for (const entry of alwaysOn) {
      if (seen.has(entry.lesson.id)) continue;
      seen.add(entry.lesson.id);
      rows.push(entry.lesson);
    }
  } else {
    warnings.push(`lesson packet: ${family} always-on query degraded`);
  }
  if (hasKeys) {
    const keyedFetch = queryRows(
      await runLessonQuery(roomDir, room, { type: family, ...keys, limit: KEYED_FETCH_LIMIT }) as Record<string, unknown>,
    );
    if (keyedFetch) {
      for (const entry of keyedFetch) {
        // Unkeyed rows are eligibility passengers in this fetch, not matches;
        // only rows that carry keys earned the ride here.
        if (!entry.keyed || seen.has(entry.lesson.id)) continue;
        seen.add(entry.lesson.id);
        rows.push(entry.lesson);
      }
    } else {
      warnings.push(`lesson packet: ${family} keyed query degraded`);
    }
  }
  return rows;
}

export async function collectLessonPacket(
  roomDir: string,
  room: string,
  work: WorkContext,
): Promise<{ lessons: PacketLesson[]; warnings: string[] }> {
  const families = work.families.length ? work.families : ["coding"];
  const keys = {
    ...(work.languageKeys.length ? { languageKeys: work.languageKeys } : {}),
    ...(work.technologyKeys.length ? { technologyKeys: work.technologyKeys } : {}),
  };
  const hasKeys = work.languageKeys.length > 0 || work.technologyKeys.length > 0;
  const warnings: string[] = [];
  const perFamily = await Promise.all(
    families.map((family) => familyLessons(roomDir, room, family, keys, hasKeys, warnings)),
  );
  // Round-robin the budget across families so a full always-on coding shelf
  // can never starve an evidence-summoned family — the failure Sol watched
  // live when Kintsu did CSS work with zero design lessons loaded.
  const chosen = perFamily.map(() => 0);
  let total = 0;
  let took = true;
  while (total < PACKET_BUDGET && took) {
    took = false;
    for (let index = 0; index < perFamily.length && total < PACKET_BUDGET; index += 1) {
      if (chosen[index] < perFamily[index].length) {
        chosen[index] += 1;
        total += 1;
        took = true;
      }
    }
  }
  const lessons = perFamily.flatMap((rows, index) => rows.slice(0, chosen[index]));
  return { lessons, warnings };
}

class LessonPacketFeedback {
  constructor(private readonly lines: string[]) {}

  render(_width: number): string[] {
    return this.lines;
  }
}

function packetLessons(details: unknown): Array<{ family: string; id: number; title: string }> {
  if (!details || typeof details !== "object" || Array.isArray(details)) return [];
  const lessons = (details as Record<string, unknown>).lessons;
  if (!Array.isArray(lessons)) return [];
  return lessons.flatMap((value) => {
    if (!value || typeof value !== "object" || Array.isArray(value)) return [];
    const lesson = value as Record<string, unknown>;
    const family = String(lesson.family ?? "").trim();
    const id = Number(lesson.id);
    const title = String(lesson.title ?? "").trim();
    return family && Number.isInteger(id) && id > 0 && title ? [{ family, id, title }] : [];
  });
}

export function lessonPacketMessageRenderer(message: any, options: any, theme: any): LessonPacketFeedback {
  const lessons = packetLessons(message?.details);
  const count = typeof message?.details?.count === "number" ? message.details.count : lessons.length;
  const summary = `Athanor · ${count || "?"} lesson${count === 1 ? "" : "s"} warm · work packet`;
  const lines = [typeof theme?.fg === "function" ? theme.fg("accent", summary) : summary];
  if (options?.expanded && lessons.length) {
    lines.push(
      "",
      ...lessons.map(({ family, id, title }) => {
        const line = `${family}#${id} — ${title}`;
        return typeof theme?.fg === "function" ? theme.fg("muted", line) : line;
      }),
    );
  }
  return new LessonPacketFeedback(lines);
}

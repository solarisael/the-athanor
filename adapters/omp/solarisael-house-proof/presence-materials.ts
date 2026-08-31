import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import path from "node:path";

import type { PresenceMaterial } from "./presence.ts";

// enough: the wake letter and the Anamnesis counsel already ride this same
// session as their own full-body reminders (solarisael-wake-context and
// solarisael-anamnesis-wake). Re-embedding them verbatim in the Presence
// frame doubled 4-6k tokens of context for the whole session, because the
// opened frame's rendered text is reused every turn. The frame's job here is
// recognition and authority, so these materials carry a bounded excerpt and
// point back at the full-body carrier — the same shape lessons already use
// (full bodies stay native, the Presence packet stays small).
export const PRESENCE_WAKE_EXCERPT_CHARS = 700;
export const PRESENCE_PULSE_MAX_CHARS = 4096;

export function presencePulseMaterial(roomDir: string): PresenceMaterial | null {
  const source = path.join(roomDir, "presence-pulse.md");
  let body: string;
  try {
    body = readFileSync(source, "utf8").trim();
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === "ENOENT") return null;
    throw error;
  }
  if (!body) return null;
  const bounded = body.length <= PRESENCE_PULSE_MAX_CHARS
    ? body
    : `${body.slice(0, PRESENCE_PULSE_MAX_CHARS)}\n[truncated at ${PRESENCE_PULSE_MAX_CHARS} characters]`;
  return {
    id: "relationship:presence-pulse",
    authority: {
      kind: "identity",
      source: "presence-pulse.md",
      sha256: createHash("sha256").update(body).digest("hex"),
    },
    role: "relationship",
    body: bounded,
    salience: 975,
  };
}

function wakeExcerpt(text: string, carrier: string): string {
  const trimmed = text.trim();
  if (trimmed.length <= PRESENCE_WAKE_EXCERPT_CHARS) return trimmed;
  const hard = trimmed.slice(0, PRESENCE_WAKE_EXCERPT_CHARS);
  const soft = hard.slice(0, hard.lastIndexOf(" ") > 0 ? hard.lastIndexOf(" ") : hard.length);
  return `${soft}\n[excerpt; the full text rides this session's ${carrier} reminder]`;
}

export function paperBoatMaterial(wake: Record<string, any>): PresenceMaterial | null {
  const memoryId = Number(wake.memoryId);
  const body = wakeExcerpt(String(wake.letter ?? ""), "solarisael-wake-context");
  if (!Number.isSafeInteger(memoryId) || memoryId <= 0 || !body) return null;
  return {
    id: `paper-boat:${memoryId}`,
    authority: { kind: "paper_boat", memory_id: memoryId },
    role: "continuity",
    body,
    salience: 900,
  };
}

export function anamnesisMaterial(content: string): PresenceMaterial[] {
  if (!content.trim()) return [];
  const body = wakeExcerpt(content, "solarisael-anamnesis-wake");
  return [{
    id: "anamnesis:wake",
    authority: { kind: "anamnesis", source: "anamnesis:wake" },
    role: "counsel",
    body,
    salience: 800,
  }];
}

export function lessonMaterials(lessons: Array<Record<string, any>>): PresenceMaterial[] {
  return lessons.flatMap((lesson) => {
    const id = Number(lesson.id);
    const body = String(lesson.body ?? "").trim();
    const version = String(lesson.version ?? "current").trim();
    if (!Number.isSafeInteger(id) || id <= 0 || !body || !version) return [];
    return [{
      id: `lesson:${id}`,
      authority: { kind: "lesson", lesson_id: id, version },
      role: "rule",
      body: body.slice(0, 4096),
      salience: 700,
    }];
  });
}

export function recallMaterials(presentation: Record<string, any>): PresenceMaterial[] {
  const memories = array(presentation.retrievalCandidates).flatMap((candidate) => {
    const memoryId = Number(candidate.memory_id ?? candidate.memoryId);
    const body = String(candidate.excerpt ?? candidate.body ?? candidate.title ?? "").trim();
    if (!Number.isSafeInteger(memoryId) || memoryId <= 0 || !body) return [];
    return [{
      id: `memory:${memoryId}`,
      authority: { kind: "memory", memory_id: memoryId },
      role: "continuity",
      body: body.slice(0, 4096),
      salience: 650,
    }];
  });
  const canon = array(presentation.canonMatches).flatMap((candidate) => {
    const entityId = String(candidate.id ?? candidate.entity_id ?? candidate.entityId ?? "").trim();
    const body = String(candidate.summary ?? candidate.name ?? "").trim();
    if (!entityId || !body) return [];
    return [{
      id: `canon:${entityId}`,
      authority: { kind: "canon", entity_id: entityId },
      role: "identity",
      body: body.slice(0, 4096),
      salience: 950,
    }];
  });
  return [...memories, ...canon].slice(0, 16) as PresenceMaterial[];
}

function array(value: unknown): Array<Record<string, any>> {
  return Array.isArray(value)
    ? value.filter((entry) => entry && typeof entry === "object" && !Array.isArray(entry))
    : [];
}

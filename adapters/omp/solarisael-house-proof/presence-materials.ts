import type { PresenceMaterial } from "./presence.ts";

export function paperBoatMaterial(wake: Record<string, any>): PresenceMaterial | null {
  const memoryId = Number(wake.memoryId);
  const body = String(wake.letter ?? "").trim();
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
  return [{
    id: "anamnesis:wake",
    authority: { kind: "anamnesis", source: "anamnesis:wake" },
    role: "counsel",
    body: content,
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

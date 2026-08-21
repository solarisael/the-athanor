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

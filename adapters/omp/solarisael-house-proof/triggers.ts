// OMP process-trigger adapter.
// The Host decides whether a matched trigger braids lessons, which lessons it
// asks for, and how the braid reads. This module runs the query it is handed.

import { hostCommand, sendHostCommand, type HostBinding } from "./host.ts";
import { runLessonQuery } from "./lesson-context.ts";

const LESSON_PLAN = "athanor.shell.lesson_plan";
const PROCESS_LESSONS = "athanor.shell.process_lessons";
const SHELL_RESULT = "athanor.shell.result";
const ACCEPTED = new Set([SHELL_RESULT]);

export type ProcessLessonReminder = { trigger: string; lessons: number; text: string };

async function shellResult(
  binding: HostBinding,
  commandType: string,
  trigger: unknown,
  lessons: unknown[] = [],
): Promise<Record<string, any>> {
  const command = hostCommand(binding, commandType, "shell", {
    trigger_request: { trigger: typeof trigger === "string" ? trigger : null, lessons },
  });
  const response = await sendHostCommand(command, ACCEPTED);
  if (!response.result || typeof response.result !== "object") {
    throw new Error("Athanor Host shell response omitted result");
  }
  return response.result;
}

export async function processLessonsReminder(
  binding: HostBinding,
  trigger: unknown,
  effectiveRoomDir: string,
  room: string,
): Promise<ProcessLessonReminder | null> {
  const { plan } = await shellResult(binding, LESSON_PLAN, trigger);
  if (!plan) return null;

  const { trigger: planned, ...filters } = plan as Record<string, unknown>;
  const query = await runLessonQuery(effectiveRoomDir, room, filters);
  const lessons = query.ok && Array.isArray(query.lessons) ? query.lessons : [];

  const { reminder } = await shellResult(binding, PROCESS_LESSONS, planned, lessons);
  return (reminder as ProcessLessonReminder) ?? null;
}

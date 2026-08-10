// OMP trigger adapter.
// Silhouette: normalize OMP messages, preserve OMP-local band state, call the shared pure core.

import { loadHouseCore } from "./core.ts";
import { conversationTurns } from "./conversation-log.ts";
import { runLessons } from "./substrate.ts";
import { messageText } from "./text.ts";
import { rankEligibleLessons, runLessonContext } from "./lesson-context.ts";
import {
  activeLessonState,
  currentLessonWorkingSet,
  rememberActiveLessonState,
  stateForLessonPrompt,
  updateLessonWorkingSet,
} from "./lesson-working-state.ts";

const nudgeBandByRoom = new Map();

function compactPayload(value) {
  if (value == null) return "";
  if (typeof value === "string") return value;
  try {
    return JSON.stringify(value);
  } catch {
    return String(value);
  }
}

function pushPayload(target, value) {
  const text = compactPayload(value).trim();
  if (text) target.push(text);
}

function pushPayloads(target, value) {
  if (Array.isArray(value)) {
    for (const item of value) pushPayload(target, item);
    return;
  }
  pushPayload(target, value);
}

function toolPartPayload(part) {
  if (!part || typeof part !== "object") return part;
  const name = part.name || part.toolName || part.tool || part.id || part.toolCallId || part.tool_call_id || part.tool_use_id;
  const args = part.args ?? part.arguments ?? part.input ?? part.parameters ?? part.result ?? part.output ?? part.content;
  if (name || args !== undefined) return { name, args };
  return part;
}

function collectToolTraffic(message) {
  const toolCalls = [];
  const toolResults = [];

  pushPayloads(toolCalls, message?.toolCalls || message?.tool_calls || message?.calls);
  pushPayloads(toolResults, message?.toolResults || message?.tool_results || message?.results);

  const parts = [
    ...(Array.isArray(message?.content) ? message.content : []),
    ...(Array.isArray(message?.parts) ? message.parts : []),
  ];

  for (const part of parts) {
    if (!part || typeof part !== "object") continue;
    const type = String(part.type || part.kind || "").toLowerCase();
    const hasToolId = Boolean(part.toolCallId || part.tool_call_id || part.tool_use_id);
    const looksLikeCall = type.includes("tool") && (type.includes("call") || type.includes("use"));
    const looksLikeResult = type.includes("tool") && (type.includes("result") || type.includes("output"));
    if (looksLikeCall || part.toolCall || part.tool_call) pushPayload(toolCalls, toolPartPayload(part));
    else if (looksLikeResult || (hasToolId && (part.result !== undefined || part.output !== undefined))) pushPayload(toolResults, toolPartPayload(part));
  }

  return { toolCalls, toolResults };
}

function normalizeOmpMessages(messages) {
  const turnsByIndex = new Map(conversationTurns(messages).map((turn) => [turn.index, turn]));
  return (Array.isArray(messages) ? messages : []).map((message, index) => {
    const turn = turnsByIndex.get(index);
    const { toolCalls, toolResults } = collectToolTraffic(message);
    const injection = (message?.role === "custom" || message?.role === "system") ? messageText(message).trim() : "";
    return {
      role: message?.role || "unknown",
      textParts: turn?.text ? [turn.text] : [],
      toolCalls,
      toolResults,
      injections: injection ? [injection] : [],
    };
  });
}

export async function contextNudge(messages, room) {
  const { computeContextNudge } = await loadHouseCore();
  const key = String(room || "room").toLowerCase();
  const lastBand = nudgeBandByRoom.get(key) || 0;
  const nudge = computeContextNudge({ messages: normalizeOmpMessages(messages), room, lastBand });
  if (!nudge) return null;
  nudgeBandByRoom.set(key, nudge.band);
  return { pct: nudge.pct, tokens: nudge.tokens, text: nudge.text };
}

export async function keywordReminder(prompt) {
  const { detectKeywordTriggers } = await loadHouseCore();
  const fired = detectKeywordTriggers(String(prompt || ""));
  if (!fired.length) return null;
  return {
    keywords: fired.map((f) => f.keyword),
    text: [
      "## Solarisael Keyword Directive",
      fired.map((f) => f.directive).join("\n\n"),
    ].join("\n"),
  };
}

export async function processLessonsReminder(prompt, effectiveRoomDir, room) {
  const { matchProcessShape, formatProcessLessonsBanner } = await loadHouseCore();
  const triggerName = matchProcessShape(String(prompt || ""));
  if (!triggerName) return null;
  const result = await runLessons(effectiveRoomDir, room, { type: "coding", shape: "process", limit: 12 });
  if (!result.ok || !Array.isArray(result.lessons) || result.lessons.length === 0) return null;
  const banner = formatProcessLessonsBanner(result.lessons, triggerName);
  return {
    trigger: triggerName,
    lessons: result.lessons.length,
    text: [
      "<system-reminder>",
      "Solarisael process-shape lessons matched this user turn.",
      "Use this as hidden reasoning context before advising on the matched process. Do not render this banner verbatim unless the operator asks.",
      "",
      banner,
      "</system-reminder>",
    ].join("\n"),
  };
}

function formatStriatumLessons(lessons) {
  const compact = lessons.slice(0, 6).map((lesson) => ({
    id: lesson.id, type: lesson.type, title: lesson.title, project: lesson.project || null,
    stage: lesson.stage || [], register: lesson.register || null,
    similarity: lesson.semantic?.similarity ?? null, lesson: lesson.lesson,
    proof_pattern: lesson.proof_pattern, trigger_context: lesson.trigger_context,
  }));
  return [
    "<system-reminder>",
    "Striatum activated authoritative lessons for the active exact project/work state.",
    `Lessons: ${JSON.stringify(compact)}`,
    "Attribution: source=lessons; ranking=Nemotron; scope/project/type/stage/register eligibility was filtered before similarity.",
    "Hidden context only; do not persist or render.",
    "</system-reminder>",
  ].join("\n");
}

/**
 * Semantic activation is deliberately downstream of exact rails. Returning
 * null is significant: the caller preserves deterministic process lessons.
 */
export async function striatumLessonsReminder(prompt, effectiveRoomDir, room) {
  const active = activeLessonState(room);
  if (!active) return null;
  const state = stateForLessonPrompt(active, String(prompt || ""));
  rememberActiveLessonState(state);
  let lessons = currentLessonWorkingSet(state);
  let refreshed = false;
  if (!lessons) {
    const context = await runLessonContext({
      effectiveRoomDir, room, projects: [state.project.project], terms: state.terms,
      stages: state.stages, registers: state.registers, limit: 48,
    });
    const eligible = [...context.codingLessons, ...context.projectLessons].slice(0, 48);
    const ranked = await rankEligibleLessons(String(prompt || state.terms.join(" ")), eligible);
    if (!ranked) return null;
    const selected = updateLessonWorkingSet(state, ranked);
    lessons = selected.lessons;
    refreshed = selected.refreshed;
  }
  if (!lessons.length) return null;
  return { lessons, refreshed, text: formatStriatumLessons(lessons) };
}

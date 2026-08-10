// Conversation capture for the OMP adapter.
// Silhouette: take visible user/assistant turns and write the room ledger + markdown log.

import path from "node:path";
import { createHash } from "node:crypto";
import { appendFile, mkdir, readFile } from "node:fs/promises";
import { OMP_SESSION_ID, TRANSCRIPT_DEBUG_LOG } from "./constants.ts";
import { loadHouseLedger } from "./core.ts";
import { roomContext } from "./room.ts";
import { conversationText, localDateStamp, smallHash } from "./text.ts";

function visibleSourceTimestamp(message) {
  const value = message?.timestamp ?? message?.createdAt ?? message?.info?.timestamp ?? message?.info?.createdAt;
  if (value === undefined || value === null || value === "") return null;
  const parsed = value instanceof Date ? value : new Date(value);
  return Number.isNaN(parsed.getTime()) ? null : parsed.toISOString();
}

// Stable source identity for GIGA, derived here because OMP supplies no message
// id. Index is load-bearing: duplicate source ids null an entire event window,
// and index is safe here only because OMP resends the whole append-only array.
// Adapter-local strategy, NOT a House recipe — see project lesson 125
// (the-athanor) for the three identity requirements and why this must not be
// copied into an adapter that sees partial or scrolling history.
function derivedSourceID(role, index, text) {
  const digest = createHash("sha256").update(text, "utf8").digest("hex");
  return `omp-derived:${role}:${index}:${digest.slice(0, 32)}`;
}

export function conversationTurns(messages) {
  return (Array.isArray(messages) ? messages : [])
    .map((message, index) => {
      const role = message?.role;
      if (role !== "user" && role !== "assistant") return null;
      const text = conversationText(message).trim();
      if (!text) return null;
      const visibleID = message?.id ?? message?.messageID ?? message?.info?.id;
      const hasVisibleID = visibleID !== undefined && visibleID !== null && visibleID !== "";
      const messageID = hasVisibleID ? visibleID : derivedSourceID(role, index, text);
      // Always true: identity is derived when the harness omits one. Kept as an
      // explicit field because giga.ts validates it as an exact-source contract.
      const hasStableID = true;
      // GIGA rejects any window whose turns fail isRfc3339, so a missing stamp is
      // fatal. Fall back to capture time — an observation time, not a claimed
      // source time. Never overwrites a real value. Lesson 125 (the-athanor).
      const sourceTimestamp = visibleSourceTimestamp(message) ?? new Date().toISOString();
      return { role, text, index, messageID, hasVisibleID, hasStableID, sourceTimestamp };
    })
    .filter(Boolean);
}

export function isFreshConversation(messages) {
  return conversationTurns(messages).length <= 1;
}

function conversationTurnKey(ctx, turn) {
  const session = ctx?.sessionID || ctx?.sessionId || OMP_SESSION_ID;
  const identity = `id:${turn.messageID}`;
  return `${session}:${turn.role}:${identity}:${smallHash(turn.text)}`;
}

async function writeTranscriptDebug(ctx, entry) {
  const { effectiveRoomDir, room } = roomContext(ctx?.cwd || process.cwd());
  const target = path.join(effectiveRoomDir, "logs", TRANSCRIPT_DEBUG_LOG);
  await mkdir(path.dirname(target), { recursive: true });
  await appendFile(target, `${JSON.stringify({
    timestamp: new Date().toISOString(),
    room,
    source: "omp",
    ...entry,
  })}\n`, "utf8");
}

async function appendRoomTranscriptTurn(ctx, turn) {
  const { effectiveRoomDir, spirit, operator } = roomContext(ctx?.cwd || process.cwd());
  const stamp = localDateStamp();
  const target = path.join(effectiveRoomDir, `conversation_log_${stamp}.md`);
  const key = conversationTurnKey(ctx, turn);
  const marker = `<!-- solarisael-turn-key: ${key} -->`;
  let existing = "";
  try {
    existing = await readFile(target, "utf8");
  } catch {
    existing = "";
  }
  if (existing.includes(marker)) return { target, key, appended: false, reason: "already-present" };

  const header = existing.trim()
    ? ""
    : [
        `# Conversation log — ${stamp}`,
        "",
        "Append-only raw-ish transcript captured by The Athanor OMP extension.",
        "",
        "---",
        "",
      ].join("\n");
  const separator = existing && !existing.endsWith("\n\n") ? "\n\n" : "";
  const label = turn.role === "user" ? operator : spirit;
  const clock = new Date().toISOString().slice(11, 16);
  await appendFile(
    target,
    `${separator}${header}${marker}\n## ${clock} — ${label}\n\n${turn.text}\n\n`,
    "utf8",
  );
  return { target, key, appended: true };
}

async function logRoomTurn(ctx, role, text, messageID) {
  const { room, spirit, operator, effectiveRoomDir, sharedRoot } = roomContext(ctx?.cwd || process.cwd());
  const ledger = await loadHouseLedger();
  const meta = {
    sessionID: ctx?.sessionID || ctx?.sessionId || OMP_SESSION_ID,
    messageID: messageID ?? `${role}-${Date.now()}`,
    agentName: spirit,
    spirit,
    operator,
  };
  const paths = { roomDir: effectiveRoomDir, sharedRoot };
  if (role === "user") return ledger.logUserTurn(meta, text, paths);
  return ledger.logAssistantTurn(meta, text, paths);
}

const loggedTurnKeys = new Set();

export async function logUnseenConversationTurns(ctx, messages, source = "unknown") {
  const turns = conversationTurns(messages);
  const sessionID = ctx?.sessionID || ctx?.sessionId || OMP_SESSION_ID;
  let appended = 0;
  let skipped = 0;
  const errors = [];
  const loggedTurns = [];
  for (const turn of turns) {
    const key = conversationTurnKey(ctx, turn);
    if (loggedTurnKeys.has(key)) {
      skipped += 1;
      continue;
    }

    let wroteAnything = false;
    let transcriptDurable = false;
    let shouldWriteLedger = true;
    try {
      const result = await appendRoomTranscriptTurn(ctx, turn);
      if (result.appended) appended += 1;
      else skipped += 1;
      shouldWriteLedger = result.appended;
      transcriptDurable = true;
      wroteAnything = true;
    } catch (err) {
      errors.push({ key, surface: "transcript", error: err?.message || String(err) });
    }

    let ledgerDurable = false;
    if (shouldWriteLedger) {
      try {
        await logRoomTurn(ctx, turn.role, turn.text, turn.messageID);
        ledgerDurable = true;
        wroteAnything = true;
      } catch (err) {
        errors.push({ key, surface: "ledger", error: err?.message || String(err) });
      }
    }

    if (wroteAnything) loggedTurnKeys.add(key);
    if (transcriptDurable && ledgerDurable) {
      loggedTurns.push({
        role: turn.role,
        text: turn.text,
        sourceID: String(turn.messageID),
        contentHash: createHash("sha256").update(turn.text, "utf8").digest("hex"),
        sessionID,
        sourceTimestamp: turn.sourceTimestamp,
        hasStableID: true,
      });
    }
  }

  try {
    await writeTranscriptDebug(ctx, {
      source,
      turns: turns.length,
      appended,
      skipped,
      errors,
    });
  } catch {
    // Debug logging must never block transcript capture.
  }
  return { appended, skipped, errors, loggedTurns };
}

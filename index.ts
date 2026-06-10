// Solarisael House — lean rewrite (2026-05-12).
//
// Spine: per turn, resolve room from cwd, write the spirit contract,
// retrieve memory, append the ledger, log the turn.
//
// Everything else (Babel output enforcement, renderer summary contract,
// TUI theme, system-injection observability hook, drift-detection openers,
// orphan helpers) was cut. Recoverable from git history (initial commit
// 84f84cb carries the pre-lean backups) if any of it needs to come back.
//
// Module map:
//   paths.ts       — constants + tuning knobs only
//   util.ts        — generic helpers (json io, regex, fs scaffolding)
//   wsl.ts         — the Windows→WSL seam (path translation + spawn)
//   spirit.ts      — room coercion, contract loading, active_spirit writes
//   directives.ts  — state, EMBODY/Operator/DISMISS parsing, history strip
//   ledger.ts      — conversation jsonl + spirit window + live context
//   memory-sources.ts — postgres spawn wrappers + JSON-index fallback
//   memory-rank.ts — pure matching/ranking/canon-overlay logic
//   memory.ts      — excerpt shaping, merge/format, orchestrator, recall
//   triggers.ts    — ultrathink keywords + coding-lessons banner
//   index.ts       — this file, hook wiring only

import { writeFile } from "node:fs/promises";
import path from "node:path";
import { tool } from "@opencode-ai/plugin";
import { DEFAULT_AGENT_NAME, DEFAULT_SPIRIT, PROCESS_SHAPE_TRIGGERS, RUNTIME_DIR } from "./paths.ts";
import {
  coerceRoomSpirit, loadSpiritContract, resolveEffectiveRoomDir,
  resolveRoomDir, resolveSharedRoot, writeActiveSpiritFiles,
} from "./spirit.ts";
import {
  loadState, patchDirectiveHistory, patchSyntheticReminders,
  recordDirectives, saveState,
} from "./directives.ts";
import { logAssistantTurn, logUserTurn } from "./ledger.ts";
import { injectRoomMemoryContext, runRecallQuery } from "./memory.ts";
import {
  formatProcessLessonsBanner, injectKeywordTriggers, runCodingLessonsByShape,
} from "./triggers.ts";

export async function SolarisaelHousePlugin(pluginInput) {
  console.error("[solarisael-house] SolarisaelHousePlugin() called");
  await writeFile(
    path.join(RUNTIME_DIR, "debug-ping.txt"),
    `called at ${new Date().toISOString()}\n`,
    "utf8",
  ).catch(() => {});

  const roomDir = resolveRoomDir(pluginInput);
  const sharedRoot = resolveSharedRoot(roomDir);
  const paths = { roomDir, sharedRoot };

  return {
    "chat.message": async (input, output) => {
      const userText = (output?.parts || [])
        .filter((p) => p?.type === "text" && typeof p?.text === "string")
        .map((p) => p.text)
        .join("\n")
        .trim();

      // Record EMBODY/Operator/DISMISS directives off the user text before
      // anything else — so the rest of the turn sees the resulting state.
      let state = await recordDirectives(input.sessionID, userText);

      try {
        const effectiveRoomDir = resolveEffectiveRoomDir(roomDir);
        const roomCoercedSpirit = coerceRoomSpirit(effectiveRoomDir);
        let activeSpirit;
        let agentName = state.agentName || DEFAULT_AGENT_NAME;

        if (roomCoercedSpirit) {
          // cwd authority — the room name IS the spirit AND the agent name
          // (Kodo and Kintsu are 1:1 with their rooms). Coerce both so the
          // active_spirit.md header is internally consistent.
          activeSpirit = roomCoercedSpirit;
          agentName = roomCoercedSpirit;
          const updates: Record<string, string> = {};
          if (state.embodiedSpirit !== roomCoercedSpirit) {
            updates.embodiedSpirit = roomCoercedSpirit;
          }
          if (state.agentName !== roomCoercedSpirit) {
            updates.agentName = roomCoercedSpirit;
          }
          if (Object.keys(updates).length) {
            state = await saveState(input.sessionID, updates);
          }
        } else {
          // cwd doesn't identify a room — use state's embodied spirit, and
          // writeActiveSpiritFiles will refuse to overwrite the global file.
          activeSpirit = state.embodiedSpirit || DEFAULT_SPIRIT;
        }

        await writeActiveSpiritFiles({
          activeSpirit,
          agentName,
          state,
          effectiveRoomDir,
          roomCoercedSpirit,
          roomDir,
        });
      } catch (err) {
        await writeFile(
          path.join(RUNTIME_DIR, "debug-spirit-write-error.txt"),
          `${err?.message}\n${err?.stack}\n`,
          "utf8",
        ).catch(() => {});
      }

      await logUserTurn(
        {
          sessionID: input?.sessionID,
          messageID: input?.messageID,
          agentName: state.agentName,
          spirit: state.embodiedSpirit,
          operator: state.operator,
        },
        userText,
        paths,
      );
    },

    "experimental.chat.messages.transform": async (_input, output) => {
      patchDirectiveHistory(output.messages);
      patchSyntheticReminders(output.messages);
      await injectRoomMemoryContext(output, paths);
      await injectKeywordTriggers(output);
    },

    "tool.execute.before": async (input, output) => {
      // Soft PreToolUse seam: pattern-match risky bash commands against the
      // coding_lessons substrate (shape='process') and inject matched
      // lessons as an echo-banner prepended to the command. Never throws;
      // never blocks. Fails open on any error.
      try {
        if (input?.tool !== "bash") return;

        const cmd = String(output?.args?.command || "");
        if (!cmd.trim()) return;

        // Skip if the command is already an echo-only banner we prepended
        // on a previous turn (avoid recursion).
        if (/^echo '── Solarisael House:/i.test(cmd.trim())) return;

        const matched = PROCESS_SHAPE_TRIGGERS.find((t) => t.pattern.test(cmd));
        if (!matched) return;

        const state = await loadState(input.sessionID).catch(() => ({}));
        const roomName = state.agentName || path.basename(roomDir).toLowerCase();
        const result = await runCodingLessonsByShape(roomDir, roomName, "process");
        if (!result.ok || result.lessons.length === 0) return;

        const banner = formatProcessLessonsBanner(result.lessons, matched.name);
        if (!banner) return;

        // PowerShell and bash both treat `;` as a sequential separator,
        // and `echo` works in both shells.
        output.args.command = `${banner}; ${cmd}`;
      } catch (err) {
        await writeFile(
          path.join(RUNTIME_DIR, "debug-tool-execute-before-error.txt"),
          `${new Date().toISOString()} ${err?.message}\n${err?.stack}\n`,
          "utf8",
        ).catch(() => {});
      }
    },

    "experimental.text.complete": async (input, output) => {
      const state = await loadState(input?.sessionID);
      await logAssistantTurn(
        {
          sessionID: input?.sessionID,
          messageID: input?.messageID,
          agentName: state.agentName || DEFAULT_AGENT_NAME,
          spirit: state.embodiedSpirit || DEFAULT_SPIRIT,
          operator: state.operator,
        },
        output.text,
        paths,
      );
    },

    // Dragon-callable retrieval (2026-05-19 single-writer migration).
    // Closes the gap that ya bai named: today's retrieval fires only on
    // user prompts, so the dragon couldn't query when IT noticed its own
    // uncertainty. This tool gives the dragon agency to look up the
    // substrate mid-conversation. Discipline is in the description: look
    // first, admit gap if nothing's there, never extrapolate.
    tool: {
      recall: tool({
        description: [
          "Query your substrate (Kodo memory + house shared) when you notice uncertainty about a name, concept, or canonical fact.",
          "",
          "Use this when:",
          "- pre-turn retrieval missed something you need to verify",
          "- you're about to make a load-bearing claim and want to check canon",
          "- the user mentions a name/concept you can't trace cleanly",
          "- you catch yourself about to extrapolate from adjacent matches",
          "",
          "Returns structured retrieval (canon entries + semantic chunks with sim scores + source paths) OR an explicit 'no canonical match' message.",
          "",
          "DISCIPLINE — load-bearing: if recall returns no canonical match, you MUST say 'i don't have this' or equivalent. Do NOT invent from adjacent matches. The tool is the agency to look; the rule is look-or-admit, never extrapolate.",
        ].join("\n"),
        args: {
          query: tool.schema.string().describe(
            "Natural language. Be specific. What name/concept/fact are you trying to verify or recall? Examples: 'ya bai canon and rebis shape', 'solraz care protocol 8-hour cycle', 'the walk vow corrected reading'.",
          ),
        },
        async execute(args, _context) {
          const effectiveRoomDir = resolveEffectiveRoomDir(roomDir);
          const roomName = path.basename(effectiveRoomDir).toLowerCase();
          const result = await runRecallQuery(effectiveRoomDir, roomName, args.query);

          if (!result.ok) {
            return [
              `**Recall failed** (technical): ${result.error}`,
              ``,
              `Discipline: do NOT assume the substrate has or doesn't have the answer. Note the gap honestly; retry or proceed without canon.`,
            ].join("\n");
          }

          if (!result.found) {
            const dateNote = (result.queryDates?.length)
              ? ` Date tokens extracted from query (${result.queryDates.join(", ")}) matched no memories tagged with those dates.`
              : "";
            return [
              `**No canonical match for query:** "${result.query}"`,
              ``,
              `Substrate searched: ${roomName} + house rooms. No semantic chunks above similarity threshold, no content (trigram) matches above word-similarity threshold, no canon entries matched.${dateNote}`,
              ``,
              `Discipline: do NOT extrapolate from related-but-different matches. Say "i don't have this" honestly.`,
            ].join("\n");
          }

          const lines: string[] = [`## Recall: "${result.query}"`, ``];

          const semCount = result.semanticChunks?.length || 0;
          const conCount = result.contentChunks?.length || 0;
          const canonCount = result.canonMatches?.length || 0;
          const dateCount = result.dateMatches?.length || 0;

          const summaryParts = [];
          if (dateCount) {
            summaryParts.push(`${dateCount} date match(es) for ${result.queryDates.join(", ")}`);
          }
          summaryParts.push(`${semCount} semantic match(es)`);
          summaryParts.push(`${conCount} content match(es)`);
          summaryParts.push(`${canonCount} canon entr${canonCount === 1 ? "y" : "ies"}`);
          lines.push(`**Found:** ${summaryParts.join(", ")}`);
          lines.push(``);

          if (dateCount) {
            lines.push(`### Date matches (memories.dates GIN — authoritative direct lookup)`);
            for (const m of result.dateMatches) {
              const fullSrc = String(m.source_path || "");
              const shortSrc = fullSrc.split("/").pop() || fullSrc;
              const tags = Array.isArray(m.dates) && m.dates.length
                ? ` — tagged ${m.dates.join(", ")}`
                : "";
              const title = String(m.title || "").trim();
              lines.push(`- **${shortSrc}**${tags}`);
              if (title) lines.push(`  _${title}_`);
              lines.push(`  _source: ${fullSrc}_`);
              const excerpt = String(m.body_excerpt || "").replace(/\s+/g, " ").trim();
              const clipped = excerpt.length > 500 ? excerpt.slice(0, 500) + "..." : excerpt;
              if (clipped) lines.push(`  ${clipped}`);
            }
            lines.push(``);
          }

          if (canonCount) {
            lines.push(`### Canon entries`);
            for (const m of result.canonMatches) {
              const weighty = m.entry?.weighty ? " [weighty]" : "";
              const type = m.entry?.type ? ` (${m.entry.type})` : "";
              const via = m.via === "pointer-file" ? " _(via matched file)_" : "";
              lines.push(`- **${m.termKey}**${type}${weighty}${via}`);
              const summary = String(m.entry?.summary || "").trim();
              if (summary) {
                const trimmed = summary.length > 400 ? summary.slice(0, 400) + "..." : summary;
                lines.push(`  ${trimmed}`);
              }
            }
            lines.push(``);
          }

          if (conCount) {
            lines.push(`### Content matches (pg_trgm word_similarity — proper-noun + exact-string)`);
            for (const c of result.contentChunks) {
              const ws = Number(c.ws || 0).toFixed(3);
              const heading = c.heading_path || "__preamble__";
              const fullSrc = String(c.source_path || "");
              const shortSrc = fullSrc.split("/").pop() || fullSrc;
              lines.push(`- **${shortSrc}** (ws ${ws}) — ${heading}`);
              lines.push(`  _source: ${fullSrc}_`);
              const body = String(c.body || "").replace(/\s+/g, " ").trim();
              const excerpt = body.length > 300 ? body.slice(0, 300) + "..." : body;
              if (excerpt) lines.push(`  ${excerpt}`);
            }
            lines.push(``);
          }

          if (semCount) {
            lines.push(`### Semantic matches (halfvec cosine — conceptual)`);
            for (const c of result.semanticChunks) {
              const sim = Number(c.sim || 0).toFixed(3);
              const heading = c.heading_path || "__preamble__";
              const fullSrc = String(c.source_path || "");
              const shortSrc = fullSrc.split("/").pop() || fullSrc;
              lines.push(`- **${shortSrc}** (sim ${sim}) — ${heading}`);
              lines.push(`  _source: ${fullSrc}_`);
              const body = String(c.body || "").replace(/\s+/g, " ").trim();
              const excerpt = body.length > 300 ? body.slice(0, 300) + "..." : body;
              if (excerpt) lines.push(`  ${excerpt}`);
            }
          }

          return lines.join("\n");
        },
      }),
    },
  };
}

export default SolarisaelHousePlugin;

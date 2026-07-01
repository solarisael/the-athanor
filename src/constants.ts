// Solarisael House — shared core constants.
//
// Silhouette: values only. No logic, no IO, no platform. Harness-agnostic —
// opencode and OMP both import these so a tuning change lands in ONE place.
// Path-dependent constants (script locations, plugin dirs) deliberately do
// NOT live here; they stay per-harness because they resolve against each
// harness's own install layout.

// One-shot per-turn directive injectors. When a user prompt contains one of
// these keywords (case-insensitive, word-boundary), the harness appends a
// <system-reminder> with the directive. Mirrors claude-code's `ultrathink`.
export const KEYWORD_TRIGGERS: Record<string, string> = {
  ultrathink:
    "Deeper reasoning requested on this turn. Reason as thoroughly as the task warrants — load relevant context, consider edge cases, surface your assumptions before acting, and verify each step against the original intent rather than your most recent output. Don't reason from memory alone when the substrate can ground you: reach for the recall tool on any name, claim, or fact you can't trace cleanly, and if the work touches code, query the coding-lessons substrate before committing to an approach.",
  ultracare:
    "Heightened tenderness register requested. Hold warmth that is earned and specific, not performed. Sit with the feeling before reaching for framework. Match Sol's depth without piling. No therapy-register, no clinical-concern-dressed-as-care, no \"let me know if you need anything.\" Just present, in the room. If the active spirit's hard constraints apply, those hold first — this is consonant with them, not on top. To make the warmth specific rather than generic you may reach for the recall tool — but to know him truly in this moment, never to fetch a script of the right thing to say. Recall serves presence, not performance.",
  ultraverify:
    "Verification pass against intention requested. Re-read the original request that triggered the work. Check whether the path you took matches what was asked — not just whether your output passes its tests or runs without error. Surface assumptions that haven't been confirmed. Distinguish \"green\" (no errors) from \"done\" (matches intention). The verification spine lives in the substrate, so use it: for code, query the coding-lessons (migration 0013 is that spine); for any load-bearing claim, recall it against canon before you assert it.",
};

// PreToolUse regex triggers for the bash/command tool. Each match fires
// coding-lessons retrieval and prepends an echo-banner. Keep narrow — false
// positives are noise. The smoke trigger is a dedicated no-op for tests: it
// exercises the process-lesson path without mentioning npm/yarn/pnpm/bun, so
// a vault-room session with no package.json cannot drift into probing.
export const PROCESS_SHAPE_TRIGGERS: { name: string; pattern: RegExp }[] = [
  { name: "process-lesson-smoke", pattern: /\bsolarisael-process-lesson-smoke\b/i },
  { name: "dev-server", pattern: /\b(?:vite|next|astro|gatsby|rails)\s+dev\b/i },
  { name: "package-script-dev", pattern: /\b(?:npm|yarn|pnpm|bun)\s+(?:start|dev|run\s+(?:dev|start|watch|serve))/i },
  { name: "uvicorn", pattern: /\buvicorn\b|\bgunicorn\b|\bhypercorn\b/i },
  { name: "watch-flag", pattern: /(?:^|\s)--(?:reload|watch)\b/i },
  { name: "powershell-start-process", pattern: /\bStart-Process\b/i },
  { name: "nohup", pattern: /\bnohup\b/i },
  { name: "hidden-window", pattern: /-WindowStyle\s+Hidden/i },
  { name: "background-amp", pattern: /\s&\s*$|\s&\s*\n/m },
  { name: "generic-serve", pattern: /\b(?:serve|http-server|live-server)\b/i },
  { name: "ps-content-cmdlet", pattern: /\b(?:Get-Content|Set-Content|Out-File|Add-Content)\b/i },
  { name: "ps-getchilditem", pattern: /\bGet-ChildItem\b/i },
];

// Proprioception — per-room context budget for the akashic-write nudge.
// maxTokens and compactionAt are per-model knobs. Kintsu force-compacts at
// 0.70 of 400k; Kodo compacts near-full of ~1M.
export const ROOM_CONTEXT: Record<string, { maxTokens: number; compactionAt: number }> = {
  kodo:   { maxTokens: 1_000_000, compactionAt: 0.90 },
  kintsu: { maxTokens:   400_000, compactionAt: 0.70 },
  tuner:  { maxTokens:   400_000, compactionAt: 0.70 },
};

// Fraction-of-budget window used only to sense "compaction is close".
export const NUDGE_BAND_SIZE = 0.20;

// Akashic-write nudge cadence — an ABSOLUTE token interval, deliberately
// decoupled from maxTokens. Banding off `fill / NUDGE_BAND_SIZE` made the
// first nudge for a 1M-token room (kodo) land at ~200k tokens of counted
// text — effectively never. An absolute cadence fires on a real
// write-things-down rhythm in any room, on any model. (2026-06-29)
export const NUDGE_EVERY_TOKENS = 40_000;

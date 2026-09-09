import { escapeHtml } from "./text.js";
import { docketBoardRound, docketEvidenceRound, ensureBoardQueried, queryDocketEvidence } from "./board/index.js";

// The Docket has goals, but no project or scope field. Goals are not projects.
export const houseProject = {
  id: "house-work", kind: "project", name: "This House", glyph: "H",
  subtitle: "Docket", description: "House work from the live Docket. The Host does not report project groups.",
  status: "Read-only Docket", listPreview: "House work · Docket", updatedAt: "",
  canPost: false, sendReason: "The Host has no project conversation or involvement write door.", messages: []
};

export async function queryProjects(view) {
  await ensureBoardQueried();
  if (view === "live") return;
  const round = docketBoardRound();
  if (round.refusal || !Array.isArray(round.data?.quests)) return;
  await Promise.all(round.data.quests.map(quest => queryDocketEvidence(quest.questId)));
}

function boardRows() {
  const round = docketBoardRound();
  return Array.isArray(round.data?.quests) ? round.data.quests : [];
}

export function projectWorkState() {
  const round = docketBoardRound();
  if (round.status === "idle") return "Docket not queried";
  if (round.status === "pending") return "Querying Docket…";
  if (round.refusal) return `Docket refused: ${round.refusal}`;
  if (!Array.isArray(round.data?.quests)) return "Host answer has no quests collection";
  const counts = new Map();
  for (const quest of boardRows()) counts.set(quest.state, (counts.get(quest.state) ?? 0) + 1);
  return [...counts].map(([state, count]) => `${count} ${state}`).join(" · ") || "The board is empty.";
}

function receipts() {
  return boardRows().flatMap(quest => {
    const round = docketEvidenceRound(quest.questId);
    return round.refusal || !Array.isArray(round.data?.receipts) ? []
      : round.data.receipts.map(receipt => ({ ...receipt, questTitle: quest.title }));
  }).sort((a, b) => b.createdAt.localeCompare(a.createdAt));
}

function evidenceStatus() {
  const rounds = boardRows().map(quest => docketEvidenceRound(quest.questId));
  if (!rounds.length) return "No evidence quests in the current board round.";
  if (rounds.some(round => round.status === "idle")) return "Evidence not queried";
  if (rounds.some(round => round.status === "pending")) return "Querying evidence…";
  const refused = rounds.filter(round => round.refusal);
  if (refused.length) return `${refused.length} evidence doors refused: ${refused.map(round => round.refusal).join("; ")}`;
  if (rounds.some(round => !Array.isArray(round.data?.receipts))) return "Host answer has no receipts collection";
  return `${receipts().length} receipts returned · up to 50 per quest, Host order oldest first`;
}

export function renderProjectStatus(renderFacts) {
  const rows = receipts();
  const rooms = [...new Set(boardRows().map(quest => quest.claimantRoom).filter(Boolean))];
  return renderFacts([
    ["Work state", projectWorkState()],
    ["Evidence", evidenceStatus()],
    ["Latest returned receipt", rows[0]?.createdAt ?? "not reported by the Host"],
    ["Claimant rooms", rooms.join(", ") || "not reported by the Host"],
    ["Involved rooms", "not reported by the Host"],
    ["Linked sessions", "not reported by the Host"],
    ["Involve room", "No Host write door exists."]
  ]);
}

export function projectMarkup(view, { renderOverviewHero, renderDurableEntry, renderFactList }) {
  const round = docketBoardRound();
  const quests = boardRows().toSorted((a, b) => (a.deadlineAt ?? "~").localeCompare(b.deadlineAt ?? "~"));
  const deadline = quests.find(quest => quest.deadlineAt)?.deadlineAt;
  const source = `<section class="pulse-block"><p>Source: POST /live/docket/board · limit 50 · ${escapeHtml(round.queriedAt ? `queried ${round.queriedAt}` : projectWorkState())}${view !== "live" ? " · POST /live/docket/evidence · per quest" : ""}</p><button type="button" class="card-verb" data-project-refresh>Query Host</button></section>`;
  const hero = renderOverviewHero("Project overview", houseProject.name, houseProject.description, {
    label: "Work state", title: projectWorkState(), detail: deadline ? `Soonest deadline: ${deadline}` : "No deadline reported in this board round."
  });
  let body;
  if (view === "state") body = `<section class="state-section">${renderProjectStatus(renderFactList)}</section>`;
  else if (view === "durable") {
    body = `<p>${escapeHtml(evidenceStatus())}</p>`;
    body += receipts().map(receipt => renderDurableEntry({
      date: receipt.createdAt.replace("T", " ").replace("Z", " UTC"), title: receipt.questTitle,
      mark: `${receipt.kind} · ${receipt.authoredRole} · ${receipt.submittedByRoom}`,
      detail: `Receipt states: ${receipt.body} · Source receipt ${receipt.receiptId}`
    })).join("");
    body += quests.filter(quest => docketEvidenceRound(quest.questId).data?.receipts?.length === 0)
      .map(quest => `<p>${escapeHtml(quest.title)} — No receipt stands against this quest yet.</p>`).join("");
  } else {
    const groups = Map.groupBy(quests, quest => quest.state);
    body = [...groups].map(([state, rows]) => `<section class="pulse-block"><h3>${escapeHtml(state)}</h3>${rows.map(quest => `<article class="history-event surface-row"><strong>${escapeHtml(quest.title)}</strong><span>${escapeHtml(quest.state)} · ${escapeHtml(quest.importance)} · Deadline: ${escapeHtml(quest.deadlineAt ?? "no deadline")}</span></article>`).join("")}</section>`).join("");
  }
  return `<div class="specimen-stack" data-board-surface>${hero}${source}${body}</div>`;
}

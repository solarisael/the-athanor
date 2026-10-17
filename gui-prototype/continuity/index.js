import { askHost } from "../health.js";
import { escapeHtml } from "../text.js";
import {
  beginSessionWorkflow, endSessionWorkflow, renderSessions,
  handleSessionsClick, handleSessionsChange, sessionWorkflowText
} from "../sessions/index.js";

let dialog;
let options;
let active = null;
let mode = "ask";
let returnFocus = null;
let returnSelector = "";
let status = { state: "idle" };
let review = null;
let sending = false;
let message = "";
const drafts = new Map();

export function initContinuity(settings) {
  options = settings;
  dialog = document.querySelector("[data-continuity-dialog]");
  dialog.addEventListener("cancel", event => {
    event.preventDefault();
    close();
  });
  dialog.addEventListener("keydown", event => event.stopPropagation());
  dialog.addEventListener("click", handleClick);
  dialog.addEventListener("input", handleInput);
  dialog.addEventListener("change", event => {
    if (event.target.matches("[data-session-select], [data-session-field]")) handleSessionsChange(event);
  });
}

export function continuityOpen() {
  return Boolean(active);
}

export function openContinuity(kind, trigger) {
  if (options.blockReason() || active || !["memory", "paper-boat", "stop"].includes(kind)) return;
  active = kind;
  returnFocus = trigger;
  returnSelector = `${trigger?.closest(".subject-header") ? ".subject-header" : ".composer"} [data-continuity-open="${kind}"]`;
  review = null;
  message = "";
  mode = "ask";
  if (!drafts.has(kind)) drafts.set(kind, { title: "", body: "", request: "", result: null, notice: "" });
  startSessionWorkflow();
  renderContinuity();
  dialog.showModal();
  dialog.querySelector("[data-continuity-close]")?.focus();
}

export function renderContinuity() {
  if (!active) return;
  const focus = document.activeElement;
  const focusId = dialog.contains(focus) ? focus.id : null;
  const cursor = focusId && typeof focus.selectionStart === "number" ? [focus.selectionStart, focus.selectionEnd] : null;
  const scroll = dialog.scrollTop;
  const blocked = options.blockReason();
  const label = active === "memory" ? "Record memory" : active === "paper-boat" ? "Fold paper boat" : "Stop";
  const markup = `<header class="continuity-heading"><h2 id="continuity-title">${label}</h2>
    <button type="button" id="continuity-close" class="card-verb" data-continuity-close>Close</button></header>
    ${blocked ? `<p role="status">${escapeHtml(blocked)}</p>` : ""}
    ${active !== "stop" ? `<nav class="continuity-actions" aria-label="Continuity author">
      <button type="button" id="continuity-ask" class="card-verb" data-continuity-mode="ask" aria-pressed="${mode === "ask"}" ${sending ? "disabled" : ""}>Ask spirit</button>
      <button type="button" id="continuity-myself" class="card-verb" data-continuity-mode="myself" aria-pressed="${mode === "myself"}" ${sending ? "disabled" : ""}>Write myself</button>
    </nav>` : ""}
    ${active === "stop" || mode === "ask" ? renderSessions({ inWorkflow: true, blockedReason: blocked }) : renderWrite(blocked)}
    ${sending ? "<p role=\"status\">A durable write awaits its receipt. Closing this window does not cancel the write.</p>" : ""}
    ${currentDraft().notice ? `<p class="continuity-feedback" role="status">${escapeHtml(currentDraft().notice)}</p>` : ""}
    ${message ? `<p class="continuity-feedback" role="status">${escapeHtml(message)}</p>` : ""}`;
  if (dialog.innerHTML !== markup) dialog.innerHTML = markup;
  dialog.scrollTop = scroll;
  if (focusId) {
    const replacement = document.getElementById(focusId);
    replacement?.focus({ preventScroll: true });
    if (cursor && replacement?.setSelectionRange) replacement.setSelectionRange(...cursor);
  }
}

function currentDraft() {
  return drafts.get(active);
}

function startSessionWorkflow() {
  const draft = currentDraft();
  if (active !== "stop" && !draft.request) {
    draft.request = active === "memory"
      ? "Please review this conversation and use remember to record the durable memory it supports. Return the canonical durable receipt. If no memory is justified, explain why."
      : "Please use sleep to write a paper boat from your actual conversation. Return the canonical durable receipt and explain any session lifecycle effects before taking them.";
  }
  beginSessionWorkflow(active === "stop" ? "interrupt" : "deliver", draft.request);
}

function saveRequest() {
  if (active && (active === "stop" || mode === "ask")) currentDraft().request = sessionWorkflowText();
}

function close() {
  saveRequest();
  active = null;
  review = null;
  endSessionWorkflow();
  dialog.close();
  options.requestRender();
  const target = returnFocus?.isConnected ? returnFocus : document.querySelector(returnSelector);
  target?.focus({ preventScroll: true });
}

function handleClick(event) {
  if (event.target.closest("[data-continuity-close]")) return close();
  const modeButton = event.target.closest("[data-continuity-mode]");
  if (modeButton && !sending && mode !== modeButton.dataset.continuityMode) {
    saveRequest();
    mode = modeButton.dataset.continuityMode;
    review = null;
    message = "";
    if (mode === "ask") startSessionWorkflow();
    else {
      endSessionWorkflow();
      void queryStatus();
    }
    renderContinuity();
    return;
  }
  const sessionButton = event.target.closest("[data-session-action]");
  if (sessionButton && ["propose", "confirm"].includes(sessionButton.dataset.sessionAction) && options.blockReason()) {
    message = options.blockReason();
    renderContinuity();
    return;
  }
  if (handleSessionsClick(event)) return;
  const action = event.target.closest("[data-continuity-action]")?.dataset.continuityAction;
  if (action === "status" && !sending) void queryStatus();
  else if (action === "review" && !sending) proposeWrite();
  else if (action === "cancel") {
    review = null;
    renderContinuity();
    dialog.querySelector("[data-continuity-action='review']")?.focus();
  } else if (action === "confirm") void confirmWrite();
  else if (action === "shelf") {
    close();
    options.openShelf();
  } else if (action === "new" && !sending) {
    currentDraft().result = null;
    review = null;
    message = "";
    renderContinuity();
  }
}

function handleInput(event) {
  if (handleSessionsChange(event)) return;
  const key = event.target.dataset.continuityField;
  if (!["title", "body"].includes(key)) return;
  currentDraft()[key] = event.target.value;
  if (review) {
    review = null;
    message = "The text changed. Review the new text before confirmation.";
    renderContinuity();
  }
}

async function queryStatus() {
  if (status.state === "pending") return;
  status = { state: "pending" };
  renderContinuity();
  try {
    const data = await askHost("/live/continuity/status");
    if (data?.ok !== true || typeof data.room !== "string" || typeof data.writesAvailable !== "boolean") {
      throw new Error("The Host returned no valid continuity status.");
    }
    status = { state: "live", data };
  } catch (error) {
    status = { state: "failed", reason: error.message };
  }
  renderContinuity();
}

function writeBlockReason() {
  if (options.blockReason()) return options.blockReason();
  if (status.state !== "live") return status.state === "failed" ? status.reason : "Read the operator and room status before writing.";
  if (!status.data.operator || !status.data.writesAvailable) return status.data.unavailableReason || "The Host does not offer operator writes.";
  return null;
}

function renderWrite(blocked) {
  const draft = currentDraft();
  const reason = blocked || writeBlockReason();
  return `<section class="settings-group">
    <h3>Operator-authored ${active === "memory" ? "memory" : "paper boat"}</h3>
    <button type="button" id="continuity-status" class="card-verb" data-continuity-action="status" ${status.state === "pending" || sending ? "disabled" : ""}>${status.state === "pending" ? "Reading status…" : "Refresh write status"}</button>
    ${status.state === "live" ? `<dl><dt>Operator</dt><dd>${escapeHtml(status.data.operator ?? "Not reported")}</dd><dt>Room</dt><dd>${escapeHtml(status.data.room)}</dd></dl>` : ""}
    ${reason ? `<p role="status">${escapeHtml(reason)}</p>` : ""}
    ${active === "memory" ? `<label for="continuity-title-input">Title<input id="continuity-title-input" data-continuity-field="title" value="${escapeHtml(draft.title)}" ${sending ? "readonly" : ""}></label>` : ""}
    <label for="continuity-body">Body<textarea id="continuity-body" data-continuity-field="body" rows="8" ${sending ? "readonly" : ""}>${escapeHtml(draft.body)}</textarea></label>
    ${draft.result ? renderDurableReceipt(draft.result) : `<button type="button" id="continuity-review" class="card-verb" data-continuity-action="review" ${reason || sending ? "disabled" : ""}>Review exact text</button>`}
    ${review ? `<section class="settings-group" tabindex="-1" id="continuity-review-panel"><h3>Confirm durable write</h3>
      <p>${escapeHtml(consequence())}</p>
      <p>Author: ${escapeHtml(review.operator)} · Room: ${escapeHtml(review.room)}</p>
      ${active === "memory" ? `<h4>${escapeHtml(review.title)}</h4>` : ""}<pre>${escapeHtml(review.body)}</pre>
      <div class="continuity-actions"><button type="button" id="continuity-cancel" class="card-verb" data-continuity-action="cancel">Cancel review</button>
      <button type="button" id="continuity-confirm" data-continuity-action="confirm" ${sending || reason ? "disabled" : ""}>${sending ? "Awaiting durable receipt…" : "Confirm durable write"}</button></div>
    </section>` : ""}
  </section>`;
}

function consequence() {
  return active === "memory"
    ? "Write this exact memory in the reported room with operator authorship."
    : "Write this exact paper boat. A new boat becomes this room's latest handoff record. An exact repeat reuses its existing record. This does not close or compact a session.";
}

function proposeWrite() {
  message = writeBlockReason() || "";
  const draft = currentDraft();
  if (!message && (!draft.body.trim() || (active === "memory" && !draft.title.trim()))) message = "Enter the required title and body.";
  if (!message) review = { kind: active, title: draft.title, body: draft.body, operator: status.data.operator, room: status.data.room };
  renderContinuity();
  if (review) document.getElementById("continuity-review-panel")?.focus();
}

async function confirmWrite() {
  if (sending || !review) return;
  const reason = writeBlockReason();
  if (reason || status.data.operator !== review.operator || status.data.room !== review.room) {
    review = null;
    message = reason || "The operator or room changed. Review the write again.";
    renderContinuity();
    return;
  }
  const command = review;
  const draft = currentDraft();
  sending = true;
  draft.notice = "";
  message = "";
  renderContinuity();
  try {
    const payload = { kind: command.kind, body: command.body, expectedOperator: command.operator };
    if (command.kind === "memory") payload.title = command.title;
    const data = await askHost("/live/continuity/write", payload);
    if (data?.ok !== true || data.kind !== command.kind || data.room !== command.room ||
        data.author?.kind !== "operator" || data.author.name !== command.operator ||
        data.receipt?.durable !== true || data.receipt.authority !== "postgres" ||
        !validMemoryId(data.receipt.memory_id)) {
      throw new Error("The Host returned no matching durable receipt.");
    }
    draft.result = data;
    review = null;
  } catch (error) {
    review = null;
    draft.notice = error.status >= 400 && error.status < 500
      ? `Write refused: ${error.message}. The draft remains available.`
      : `No durable receipt: ${error.message}. The write outcome is not confirmed here. Check the memory shelf before another write. The draft remains available.`;
  }
  sending = false;
  renderContinuity();
  document.getElementById("continuity-durable-receipt")?.focus();
}

function validMemoryId(id) {
  if (typeof id === "string") return /^[1-9]\d*$/.test(id);
  return Number.isSafeInteger(id) && id > 0;
}

function renderDurableReceipt(data) {
  return `<section id="continuity-durable-receipt" tabindex="-1" class="settings-group"><h3>Canonical durable receipt</h3>
    <p>Author: ${escapeHtml(data.author.name)} · Room: ${escapeHtml(data.room)}</p>
    ${data.receipt.inserted === false ? "<p role=\"status\">Existing record reused. No new record was created.</p>" : ""}
    <pre>${escapeHtml(JSON.stringify(data.receipt, null, 2))}</pre>
    <div class="continuity-actions"><button type="button" class="card-verb" data-continuity-action="shelf">Open room memories</button>
    <button type="button" class="card-verb" data-continuity-action="new">Review another write</button></div></section>`;
}

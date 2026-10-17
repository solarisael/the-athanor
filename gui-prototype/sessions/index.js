import { askHost } from "../health.js";
import { escapeHtml } from "../text.js";

const ROUTE = "/live/session";
const CONTROLS = ["deliver", "interrupt", "compact", "close", "restart"];
const PENDING_STATES = ["pending", "queued", "delivered"];
const TERMINAL_STATES = ["applied", "refused", "failed", "expired", "outcomeUnknown"];
const LABELS = { deliver: "Deliver text", interrupt: "Interrupt", compact: "Compact", close: "Close", restart: "Restart" };

let requestRender = () => {};
let round = { status: "idle" };
let selection = null;
let draft = emptyDraft();
let proposal = null;
let notice = "";
let submitting = false;
const receipts = new Map();
let workflow = null;

export function beginSessionWorkflow(kind, text = "") {
  workflow = { kind };
  selection = null;
  draft = { ...emptyDraft(), text };
  proposal = null;
  notice = "";
  void queryStatus();
}

export function endSessionWorkflow() {
  workflow = null;
  proposal = null;
}

export function sessionWorkflowText() {
  return draft.text;
}

export function initSessions(options) {
  requestRender = options.requestRender;
}

export function ensureSessionsQueried() {
  if (round.status === "idle") return queryStatus();
}

export function renderSessions({ inWorkflow = false, blockedReason = null } = {}) {
  if (workflow && !inWorkflow) return "";
  if (workflow && inWorkflow) workflow.blockedReason = blockedReason;
  const pending = round.status === "pending";
  const session = selectedSession();
  return `<section class="mechanics-observatory" data-sessions-surface aria-labelledby="sessions-title">
    <header class="mechanics-lead">
      <h2 id="sessions-title">Live sessions</h2>
      <p>Exact instances registered with this room's Host.</p>
      <p>Room chat target: ${escapeHtml(round.data?.chatDeliveryOwner ?? "not reported")}. Session controls do not change room chat routing.</p>
      <button type="button" id="session-refresh" class="card-verb" data-session-action="refresh" aria-disabled="${pending}">${pending ? "Querying status…" : "Refresh session status"}</button>
      ${round.queriedAt ? `<p>Last status response: ${escapeHtml(round.queriedAt)}</p>` : ""}
    </header>
    ${renderStatus()}
    ${round.status === "live" ? renderSelection() : ""}
    ${session ? renderTarget(session) : ""}
    ${notice ? `<p role="status">${escapeHtml(notice)}</p>` : ""}
    ${proposal ? renderConfirmation() : ""}
    ${renderReceipts()}
  </section>`;
}

export function handleSessionsClick(event) {
  const button = event.target.closest("[data-session-action]");
  if (!button || !button.closest("[data-sessions-surface]")) return false;
  event.preventDefault();
  if (button.disabled || button.getAttribute("aria-disabled") === "true") return true;
  readFields(button.closest("[data-sessions-surface]"));

  const action = button.dataset.sessionAction;
  if (action === "refresh") void queryStatus();
  else if (action === "propose") propose(button.dataset.sessionKind);
  else if (action === "cancel") {
    const kind = proposal?.kind;
    proposal = null;
    requestRender();
    document.querySelector(`[data-session-action="propose"][data-session-kind="${kind}"]`)?.focus({ preventScroll: true });
  } else if (action === "confirm") void confirmCommand();
  else if (action === "receipt") void queryReceipt(button.dataset.commandId);
  return true;
}

export function handleSessionsChange(event) {
  const field = event.target;
  if (!field.closest("[data-sessions-surface]")) return false;
  if (field.matches("[data-session-select]")) {
    const session = round.status === "live" ? round.data.sessions.find(row => row.instanceId === field.value) : null;
    selection = session ? { instanceId: session.instanceId, conversationId: session.conversationId } : null;
    if (!workflow) draft = emptyDraft();
    proposal = null;
    notice = "";
    requestRender();
    return true;
  }
  if (!field.matches("[data-session-field]")) return false;
  readFields(field.closest("[data-sessions-surface]"));
  if (proposal) {
    proposal = null;
    notice = "The command fields changed. Review a new proposal before confirmation.";
    requestRender();
  }
  return true;
}

async function queryStatus() {
  if (round.status === "pending") return;
  proposal = null;
  round = { ...round, status: "pending" };
  requestRender();
  try {
    const data = await askHost(ROUTE, { action: "status" });
    assertAnswer(data);
    if (!Array.isArray(data.sessions) || data.sessions.some(session => !session ||
      typeof session.instanceId !== "string" || !session.instanceId ||
      typeof session.conversationId !== "string" || !session.conversationId)) {
      throw new Error("The Host response has no valid instance and conversation collection.");
    }
    round = { status: "live", data, queriedAt: new Date().toISOString() };
    if (selection && !selectedSession()) {
      selection = null;
      if (!workflow) draft = emptyDraft();
      notice = "The selected instance and conversation are no longer registered. Select a target again.";
    }
  } catch (error) {
    round = { status: errorStatus(error), reason: error.message };
  }
  requestRender();
}

function renderStatus() {
  if (round.status === "idle") return "<p role=\"status\">Session status has not been queried.</p>";
  if (round.status === "pending") return "<p role=\"status\">Reading session status… Controls are unavailable during this query.</p>";
  if (round.status !== "live") return `<p role="status">Session status ${round.status === "refused" ? "refused" : "failed"}: ${escapeHtml(round.reason)}</p>`;
  if (!round.data.sessions.length) return "<p role=\"status\">The Host reports no registered sessions.</p>";
  return `<p role="status">${round.data.sessions.length} registered instance(s) · Room ${escapeHtml(round.data.room)} · Spirit ${escapeHtml(round.data.spirit)}</p>`;
}

function renderSelection() {
  if (!round.data.sessions.length) return "";
  return `<div class="settings-group">
    <h3>Choose an exact target</h3>
    <label for="session-instance">Instance and conversation
      <select id="session-instance" data-session-select>
        <option value=""${selection ? "" : " selected"}>No instance selected</option>
        ${round.data.sessions.map(session => `<option value="${escapeHtml(session.instanceId)}"${matchesSelection(session) ? " selected" : ""}>${escapeHtml(session.instanceId)} · ${escapeHtml(session.conversationId)} · ${connectionState(session)}</option>`).join("")}
      </select>
    </label>
    ${selection ? "" : "<p>Select an instance to see its offered controls.</p>"}
  </div>`;
}

function renderTarget(session) {
  const offered = availableControls(session);
  const controls = workflow ? offered.filter(kind => kind === workflow.kind) : offered;
  const unavailable = (workflow ? [workflow.kind] : CONTROLS).filter(kind => !controls.includes(kind));
  return `<section class="settings-group" aria-labelledby="session-target-title">
    <h3 id="session-target-title">Selected target</h3>
    <dl>
      <dt>Instance</dt><dd><code>${escapeHtml(session.instanceId)}</code></dd>
      <dt>Conversation</dt><dd><code>${escapeHtml(session.conversationId)}</code></dd>
      <dt>Connection</dt><dd>${connectionState(session)}</dd>
      <dt>Process observation</dt><dd>${session.processObserved === true ? "Observed" : "Not observed"}</dd>
      <dt>Lease expires</dt><dd>${escapeHtml(session.leaseExpiresAt ?? "not reported")}</dd>
      <dt>Attached</dt><dd>${escapeHtml(session.attachedAt ?? "not reported")}</dd>
      <dt>Room chat delivery</dt><dd>${session.chatDelivery === true ? "Current owner" : "Not the current owner"}</dd>
      <dt>Offered controls</dt><dd>${escapeHtml(Array.isArray(session.controls) ? session.controls.join(", ") || "none" : "not reported")}</dd>
    </dl>
    ${session.connected !== true ? "<p>Controls are unavailable: this instance has no live connection.</p>" : ""}
    ${unavailable.length ? `<p>Unavailable controls: ${escapeHtml(unavailable.join(", "))}.</p>` : ""}
    ${session.controls?.includes("restart") && !controls.includes("restart") ? `<p>Restart unavailable: ${escapeHtml(restartReason(session))}</p>` : ""}
    ${controls.includes("deliver") ? `<label for="session-delivery-text">Operator text
      <textarea id="session-delivery-text" data-session-field="text" rows="5">${escapeHtml(draft.text)}</textarea>
    </label>
    <label for="session-delivery-mode">Delivery mode
      <select id="session-delivery-mode" data-session-field="deliveryMode">
        ${option("steer", "Steer", draft.deliveryMode)}${option("followUp", "Follow up", draft.deliveryMode)}
      </select>
    </label>` : ""}
    ${controls.includes("compact") ? `<label for="session-compact-instructions">Compaction instructions (optional)
      <textarea id="session-compact-instructions" data-session-field="instructions" rows="3">${escapeHtml(draft.instructions)}</textarea>
    </label>` : ""}
    ${controls.some(kind => kind === "close" || kind === "restart") ? `<label for="session-command-reason">Reason (required for restart)
      <input id="session-command-reason" type="text" data-session-field="reason" value="${escapeHtml(draft.reason)}">
    </label>` : ""}
    ${controls.includes("restart") ? `<label for="session-restart-mode">Restart mode
      <select id="session-restart-mode" data-session-field="restartMode">
        ${option("resume", "Resume the saved conversation", draft.restartMode)}${option("fresh", "Start a fresh conversation", draft.restartMode)}
      </select>
    </label>` : ""}
    <div class="mechanics-snapshot-status">${controls.map(kind => `<button type="button" id="session-propose-${kind}" class="card-verb" data-session-action="propose" data-session-kind="${kind}" aria-disabled="${submitting || Boolean(workflow?.blockedReason)}">Review ${LABELS[kind].toLowerCase()}</button>`).join("")}</div>
    ${workflow?.kind === "deliver" ? "<p>Delivery requests a spirit action. A command receipt is not a durable memory or paper-boat receipt.</p>" : ""}
    ${submitting ? "<p role=\"status\">A command request is in flight. Wait for its receipt before another command.</p>" : ""}
  </section>`;
}

function propose(kind) {
  if (workflow && kind !== workflow.kind) return;
  const session = selectedSession();
  notice = "";
  proposal = null;
  if (!session || !availableControls(session).includes(kind)) {
    notice = "The selected instance does not currently offer that control. Refresh status before another proposal.";
  } else {
    const payload = payloadFor(kind);
    const reason = payloadProblem(kind, payload);
    if (reason) notice = reason;
    else proposal = { kind, payload, instanceId: session.instanceId, conversationId: session.conversationId };
  }
  requestRender();
  if (proposal) document.querySelector("[data-session-confirm]")?.focus();
}

function renderConfirmation() {
  const disabled = submitting || Boolean(workflow?.blockedReason) || typeof globalThis.crypto?.randomUUID !== "function";
  return `<section class="settings-group" id="session-confirmation" aria-labelledby="session-confirm-title" data-session-confirm tabindex="-1">
    <h3 id="session-confirm-title">Confirm ${escapeHtml(LABELS[proposal.kind].toLowerCase())}</h3>
    <p class="sediment-authority">${escapeHtml(effect(proposal))}</p>
    <dl><dt>Exact instance</dt><dd><code>${escapeHtml(proposal.instanceId)}</code></dd>
      <dt>Exact conversation</dt><dd><code>${escapeHtml(proposal.conversationId)}</code></dd></dl>
    ${Object.entries(proposal.payload).map(([key, value]) => `<div class="pulse-block"><strong>${escapeHtml(key)}</strong><pre>${escapeHtml(value)}</pre></div>`).join("")}
    <p>This confirmation sends one command to this instance and conversation.</p>
    <button type="button" id="session-cancel" class="card-verb" data-session-action="cancel">Cancel proposal</button>
    <button type="button" id="session-confirm" class="card-verb" data-session-action="confirm" aria-disabled="${disabled}">Confirm ${escapeHtml(LABELS[proposal.kind].toLowerCase())} for this exact target</button>
    ${disabled ? `<p>${escapeHtml(workflow?.blockedReason || "Confirmation is unavailable while sending or without secure command ID support.")}</p>` : ""}
  </section>`;
}

async function confirmCommand() {
  if (submitting || !proposal) return;
  const session = selectedSession();
  if (!session || session.instanceId !== proposal.instanceId || session.conversationId !== proposal.conversationId ||
    !availableControls(session).includes(proposal.kind) || typeof globalThis.crypto?.randomUUID !== "function") {
    proposal = null;
    notice = "The target or its control is unavailable. Refresh status and review a new proposal.";
    requestRender();
    return;
  }

  const command = proposal;
  const commandId = globalThis.crypto.randomUUID();
  const receipt = { ...command, commandId, state: "requesting", checking: false, continuityRequest: workflow?.kind === "deliver" };
  receipts.set(commandId, receipt);
  proposal = null;
  submitting = true;
  requestRender();
  document.getElementById(`session-receipt-${commandId}`)?.focus();
  try {
    const data = await askHost(ROUTE, {
      action: "command", commandId,
      targetInstanceId: command.instanceId,
      targetConversationId: command.conversationId,
      kind: command.kind, payload: command.payload
    });
    updateReceipt(receipt, data);
  } catch (error) {
    receipt.state = errorStatus(error) === "refused" ? "requestRefused" : "receiptUnconfirmed";
    receipt.error = error.message;
  }
  submitting = false;
  requestRender();
}

async function queryReceipt(commandId) {
  const receipt = receipts.get(commandId);
  if (!receipt || receipt.checking || receipt.state === "requesting") return;
  receipt.checking = true;
  receipt.error = "";
  requestRender();
  try {
    const data = await askHost(ROUTE, { action: "commandStatus", commandId });
    updateReceipt(receipt, data);
  } catch (error) {
    receipt.error = `Receipt query ${errorStatus(error) === "refused" ? "refused" : "failed"}: ${error.message}`;
  }
  receipt.checking = false;
  requestRender();
}

function updateReceipt(receipt, data) {
  assertAnswer(data);
  if (data.commandId !== receipt.commandId || ![...PENDING_STATES, ...TERMINAL_STATES].includes(data.state) ||
    (data.targetInstanceId !== undefined && data.targetInstanceId !== receipt.instanceId) ||
    (data.kind !== undefined && data.kind !== receipt.kind)) {
    throw new Error("The Host response does not match this command receipt.");
  }
  receipt.state = data.state;
  receipt.detail = data.detail;
  receipt.restartId = data.restartId;
  receipt.deadlineAt = data.deadlineAt ?? receipt.deadlineAt;
  receipt.error = "";
}

function renderReceipts() {
  if (!receipts.size) return "";
  return `<section class="pulse-block" aria-labelledby="session-receipts-title">
    <h3 id="session-receipts-title">Command receipts</h3>
    ${[...receipts.values()].toReversed().map(receipt => {
      const pending = PENDING_STATES.includes(receipt.state);
      const terminal = TERMINAL_STATES.includes(receipt.state);
      const restart = round.status === "live" && receipt.restartId && Array.isArray(round.data.restarts)
        ? round.data.restarts.find(row => row.restartId === receipt.restartId) : null;
      return `<article class="settings-group" id="session-receipt-${escapeHtml(receipt.commandId)}" tabindex="-1">
        <h4>${escapeHtml(LABELS[receipt.kind])} · ${escapeHtml(receipt.state)}</h4>
        <p role="status">${pending ? "Pending Host receipt" : terminal ? "Terminal Host receipt" : "Request outcome"}: ${escapeHtml(receipt.state)}</p>
        ${receipt.continuityRequest ? "<p>Delivery receipt only. Check the selected conversation for the spirit's durable receipt.</p>" : ""}
        ${receipt.state === "receiptUnconfirmed" ? "<p>The command may have reached the Host. Check this command ID. No command is sent again automatically.</p>" : ""}
        ${receipt.state === "outcomeUnknown" ? "<p>The Host cannot establish the command outcome. This receipt does not prove that no effect occurred.</p>" : ""}
        <dl><dt>Command</dt><dd><code>${escapeHtml(receipt.commandId)}</code></dd>
          <dt>Instance</dt><dd><code>${escapeHtml(receipt.instanceId)}</code></dd>
          <dt>Conversation at issue</dt><dd><code>${escapeHtml(receipt.conversationId)}</code></dd>
          ${receipt.deadlineAt ? `<dt>Deadline</dt><dd>${escapeHtml(receipt.deadlineAt)}</dd>` : ""}
          ${receipt.restartId ? `<dt>Restart</dt><dd><code>${escapeHtml(receipt.restartId)}</code></dd>` : ""}
          ${restart ? `<dt>Restart state from last status</dt><dd>${escapeHtml(restart.state)}</dd><dt>Successor instance</dt><dd>${escapeHtml(restart.successorInstanceId ?? "not reported")}</dd>` : ""}
        </dl>
        ${receipt.error ? `<p role="status">${escapeHtml(receipt.error)}</p>` : ""}
        ${receipt.detail || restart?.detail ? `<details><summary>Host receipt detail</summary><p>${escapeHtml(receipt.detail ?? "")}</p><p>${escapeHtml(restart?.detail ?? "")}</p></details>` : ""}
        <button type="button" id="session-check-${escapeHtml(receipt.commandId)}" class="card-verb" data-session-action="receipt" data-command-id="${escapeHtml(receipt.commandId)}" aria-disabled="${receipt.checking || receipt.state === "requesting"}">${receipt.checking ? "Checking receipt…" : receipt.state === "requesting" ? "Awaiting command response…" : "Check command receipt"}</button>
      </article>`;
    }).join("")}
  </section>`;
}

function selectedSession() {
  return round.status === "live" && selection ? round.data.sessions.find(matchesSelection) : null;
}

function matchesSelection(session) {
  return selection?.instanceId === session.instanceId && selection.conversationId === session.conversationId;
}

function availableControls(session) {
  if (session.connected !== true || session.processExited === true || !Array.isArray(session.controls)) return [];
  return CONTROLS.filter(kind => session.controls.includes(kind) && !session.unsupportedControls?.includes(kind) &&
    (kind !== "restart" || (session.launcher?.restart === true && round.data.restartAuthority === true)));
}

function restartReason(session) {
  if (session.connected !== true) return "the instance has no live connection.";
  if (round.data.restartAuthority !== true) return "the Host does not report restart authority.";
  return session.launcher?.reason ?? "the Host does not offer a supported restart launcher.";
}

function connectionState(session) {
  if (session.processExited === true) return "Process exited";
  if (session.connected === true) return "Connected";
  if (session.connected === false) return "Disconnected";
  return "Connection not reported";
}

function emptyDraft() {
  return { text: "", deliveryMode: "followUp", instructions: "", reason: "", restartMode: "resume" };
}

function readFields(surface) {
  for (const field of surface.querySelectorAll("[data-session-field]")) {
    if (Object.hasOwn(draft, field.dataset.sessionField)) draft[field.dataset.sessionField] = field.value;
  }
}

function payloadFor(kind) {
  if (kind === "deliver") return { text: draft.text, mode: draft.deliveryMode };
  if (kind === "compact") return draft.instructions ? { instructions: draft.instructions } : {};
  if (kind === "close") return draft.reason ? { reason: draft.reason } : {};
  if (kind === "restart") return { reason: draft.reason, mode: draft.restartMode };
  return {};
}

function payloadProblem(kind, payload) {
  const bytes = value => new TextEncoder().encode(value).length;
  if (kind === "deliver" && (!payload.text.trim() || bytes(payload.text) > 65536)) return "Delivery requires operator text, up to 65536 UTF-8 bytes.";
  if (kind === "deliver" && !["steer", "followUp"].includes(payload.mode)) return "Select an offered delivery mode.";
  if (kind === "compact" && payload.instructions && bytes(payload.instructions) > 4096) return "Compaction instructions must not exceed 4096 UTF-8 bytes.";
  if (kind === "restart" && (!payload.reason.trim() || bytes(payload.reason) > 1024)) return "Restart requires a reason, up to 1024 UTF-8 bytes.";
  if (kind === "restart" && !["resume", "fresh"].includes(payload.mode)) return "Select an offered restart mode.";
  return "";
}

function effect(command) {
  if (command.kind === "deliver") return `Deliver the operator text to the selected conversation in ${command.payload.mode === "steer" ? "steer" : "follow-up"} mode.`;
  if (command.kind === "interrupt") return "Interrupt the active turn in this exact instance.";
  if (command.kind === "compact") return "Compact the context of this exact conversation, with the instructions shown below when supplied.";
  if (command.kind === "close") return "Request cooperative shutdown of this exact instance. This command does not restart it.";
  return command.payload.mode === "fresh"
    ? "Request shutdown of this exact instance, then start a fresh conversation through its registered launcher. The saved conversation is not resumed."
    : "Request shutdown of this exact instance, then resume its saved conversation through its registered launcher.";
}

function assertAnswer(data) {
  if (data?.ok === false) {
    throw Object.assign(new Error(data.error?.message ?? "The Host refused the session request."), { status: 400 });
  }
  if (data?.ok !== true) throw new Error("The Host response has no session result marker.");
}

function errorStatus(error) {
  return error.status >= 400 && error.status < 500 ? "refused" : "failed";
}

function option(value, label, selected) {
  return `<option value="${value}"${value === selected ? " selected" : ""}>${label}</option>`;
}

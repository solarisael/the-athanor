import { askHost, queryHealthHost } from "./health.js";
import { escapeHtml } from "./text.js";

let round = { status: "idle", matrix: null, result: null };
let requestRender = () => {};

export function initRepair(options) {
  requestRender = options.requestRender;
}

export async function queryRepairStatus() {
  if (round.status === "pending") return;
  round = { ...round, status: "pending", result: null };
  requestRender();
  try {
    const matrix = await askHost("/local/repair/status");
    round = { ...round, status: "answered", matrix };
  } catch (error) {
    round = { ...round, status: "failed", result: { error: error.message } };
  }
  requestRender();
}

export async function startRepair(service = false) {
  if (round.status === "pending") return;
  round = { ...round, status: "pending", result: null };
  requestRender();
  let result;
  try {
    result = await askHost("/local/repair/start", { service });
  } catch (error) {
    result = { error: error.message };
  }
  try {
    const matrix = await askHost("/local/repair/status");
    round = { status: result.error ? "failed" : "answered", matrix, result };
  } catch (error) {
    round = { ...round, status: "failed", result: { ...result, error: [result.error, error.message].filter(Boolean).join(" · ") } };
  }
  requestRender();
  await queryHealthHost();
}

export function renderRepair() {
  const disabled = round.status === "pending" ? " disabled" : "";
  const rows = (round.matrix?.components ?? []).map(component => {
    const dots = ["installed", "running", "reachable", "healthy"].map(field => {
      const value = component[field];
      const label = `${field}: ${value === true ? "yes" : value === false ? "no" : "not reported"}`;
      return `<span class="repair-dot" title="${label}" role="img" aria-label="${label}">${value === true ? "●" : value === false ? "○" : "◌"}</span>`;
    }).join(" ");
    return `<li><strong>${escapeHtml(component.name)}</strong> · <span class="repair-dots">${dots}</span> · <span>${escapeHtml(component.detail)}</span></li>`;
  }).join("");
  const result = round.result;
  const message = result?.error ?? (result ? [
    result.started?.length ? `Started: ${result.started.join(", ")}` : "",
    result.skipped?.length ? `Skipped: ${result.skipped.join(", ")}` : "",
    ...(result.refused ?? []).map(item => `${item.component}: ${item.reason}`),
    result.exitCode != null ? `Exit ${result.exitCode}` : ""
  ].filter(Boolean).join(" · ") : "");
  return `<button type="button" data-repair="status"${disabled}>Repair connection</button>
    ${rows ? `<ul class="repair-matrix">${rows}</ul>` : ""}
    ${round.matrix ? `<button type="button" data-repair="start"${disabled}>Start what is missing</button>` : ""}
    ${round.matrix?.elevationRequired?.includes("service") ? `<button type="button" data-repair="service"${disabled}>Start service (asks for administrator)</button>` : ""}
    ${message ? `<p class="repair-result" role="status">${escapeHtml(message)}</p>` : ""}`;
}

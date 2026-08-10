const isRecord = value => !!value && typeof value === 'object' && !Array.isArray(value);

export function operationId(scope, index) { return `operation-${scope}-${index}`; }

function diagnosticFrom(payload) {
  if (!isRecord(payload)) return undefined;
  if (isRecord(payload.error)) return payload.error;
  if (isRecord(payload.result) && (typeof payload.result.code === 'string' || isRecord(payload.result.details))) return payload.result;
  return typeof payload.code === 'string' || isRecord(payload.details) ? payload : undefined;
}

function evidenceWarning(evidence) {
  if (!isRecord(evidence) || !['warning', 'warn', 'degraded'].includes(String(evidence.severity ?? evidence.level).toLowerCase())) return undefined;
  return typeof evidence.message === 'string' ? evidence.message : typeof evidence.summary === 'string' ? evidence.summary : 'Diagnostic evidence reports a warning.';
}

export function diagnosticView(payload) {
  const diagnostic = diagnosticFrom(payload);
  const details = isRecord(diagnostic?.details) ? diagnostic.details : {};
  const execution = isRecord(details.execution) ? details.execution : {};
  const evidence = Array.isArray(details.evidence) ? details.evidence : [];
  const warnings = evidence.map(evidenceWarning).filter(Boolean);
  if (diagnostic?.retryable) warnings.push('This failure is marked retryable.');
  if (typeof execution.retry === 'string' && execution.retry !== 'never') warnings.push(`Retry: ${execution.retry.replaceAll('_', ' ')}.`);
  const unknown = diagnostic?.code === 'AUTHORITATIVE_OUTCOME_UNKNOWN' || execution.write_outcome === 'unknown';
  if (unknown) warnings.unshift('The write outcome is unknown. Reconcile before retrying.');
  const failed = isRecord(payload) && payload.ok === false || !!diagnostic && !isRecord(payload?.result);
  const kind = unknown ? 'unknown' : failed ? 'failed' : warnings.length ? 'degraded' : 'success';
  const code = typeof diagnostic?.code === 'string' ? diagnostic.code : undefined;
  const message = typeof diagnostic?.message === 'string' ? diagnostic.message : undefined;
  const summary = unknown ? 'Outcome unknown — reconcile before retrying.' : failed ? `${code ?? 'Request failed'}${message ? `: ${message}` : ''}` : kind === 'degraded' ? 'Completed with warnings.' : 'Completed successfully.';
  const rawPacket = diagnostic ?? payload;
  return { kind, summary, warnings, rawPacket, rawText: JSON.stringify(rawPacket, null, 2), details };
}

export function createOperationTarget(documentRef, host, id) {
  const container = documentRef.createElement('div');
  container.className = 'operation-status';
  container.id = id;
  container.dataset.state = 'idle';
  const status = documentRef.createElement('p');
  status.className = 'operation-summary';
  status.setAttribute('role', 'status');
  const warnings = documentRef.createElement('ul');
  warnings.className = 'operation-warnings';
  const raw = documentRef.createElement('details');
  raw.className = 'raw-diagnostic';
  const summary = documentRef.createElement('summary');
  summary.textContent = 'Raw diagnostic';
  const copy = documentRef.createElement('button');
  copy.type = 'button';
  copy.className = 'copy-diagnostic';
  copy.textContent = 'Copy raw JSON';
  const pre = documentRef.createElement('pre');
  pre.className = 'raw-diagnostic-packet';
  raw.append(summary, copy, pre);
  container.append(status, warnings, raw);
  host.after(container);
  return { container, status, warnings, raw, copy, pre };
}

export function renderOperation(target, payload) {
  const view = diagnosticView(payload);
  target.container.dataset.state = view.kind;
  target.status.textContent = view.summary;
  target.warnings.replaceChildren(...view.warnings.map(message => {
    const item = target.warnings.ownerDocument.createElement('li');
    item.textContent = message;
    return item;
  }));
  target.pre.textContent = view.rawText;
  target.copy.onclick = async () => {
    try {
      await navigator.clipboard.writeText(view.rawText);
      target.copy.textContent = 'Copied raw JSON';
    } catch {
      target.copy.textContent = 'Copy unavailable';
    }
  };
  return view;
}

function setBusy(form, target, busy) {
  target.container.dataset.state = busy ? 'busy' : target.container.dataset.state;
  if (busy) target.status.textContent = 'Working…';
  for (const control of form.elements) if ('disabled' in control) control.disabled = busy;
}

function formPayload(form) {
  const payload = {};
  for (const element of form.elements) if (element.name && (element.type !== 'checkbox' || element.checked)) payload[element.name] = element.type === 'number' ? Number(element.value) : element.type === 'checkbox' ? true : element.value;
  return payload;
}

function writeOutput(documentRef, id, value) {
  const output = documentRef.getElementById(id);
  if (output) output.textContent = typeof value === 'string' ? value : JSON.stringify(value, null, 2);
}

async function post(path, payload, token) {
  const response = await fetch(path, { method: 'POST', headers: { 'content-type': 'application/json', 'x-csrf-token': token }, body: JSON.stringify(payload) });
  return response.json().catch(() => ({ ok: false, error: { code: 'INVALID_GUI_RESPONSE', message: `HTTP ${response.status}`, retryable: false } }));
}

function localFailure(error) {
  return { ok: false, error: { code: 'GUI_REQUEST_FAILED', message: error instanceof Error ? error.message : String(error), retryable: false } };
}

function bindOperation(documentRef, host, id, invoke, controls, onComplete) {
  const target = createOperationTarget(documentRef, host, id);
  const run = async () => {
    controls.forEach(control => { control.disabled = true; });
    target.container.dataset.state = 'busy';
    target.status.textContent = 'Working…';
    try {
      const result = await invoke();
      renderOperation(target, result);
      onComplete?.(result);
    } catch (error) { renderOperation(target, localFailure(error)); }
    finally { controls.forEach(control => { control.disabled = false; }); }
  };
  return { target, run };
}

export function initializeGui(documentRef = document) {
  let token = '';
  const status = documentRef.getElementById('status');
  fetch('/api/csrf').then(response => response.json()).then(value => { token = value.token; sessionStorage.setItem('house-csrf', token); }).catch(error => { status.textContent = `Unavailable: ${error.message}`; });
  fetch('/api/health').then(async response => ({ response, payload: await response.json() })).then(({ response, payload }) => { const view = diagnosticView(payload); status.textContent = response.ok ? 'Ready' : view.summary; }).catch(error => { status.textContent = `Unavailable: ${error.message}`; });
  fetch('/api/backups').then(response => response.json()).then(value => writeOutput(documentRef, 'backup-results', value)).catch(error => writeOutput(documentRef, 'backup-results', error.message));

  [...documentRef.querySelectorAll('form[data-method]')].forEach((form, index) => {
    const method = form.dataset.method;
    const operation = bindOperation(documentRef, form, operationId(`rpc-${method}`, index), () => post('/api/rpc', { method, params: formPayload(form) }, token), [...form.elements]);
    form.addEventListener('submit', async event => {
      event.preventDefault();
      setBusy(form, operation.target, true);
      try {
        const result = await post('/api/rpc', { method, params: formPayload(form) }, token);
        renderOperation(operation.target, result);
        if (result.ok) writeOutput(documentRef, method === 'recall' ? 'recall-results' : method === 'anamnesis' ? 'anamnesis-results' : '', result.result);
      } catch (error) { renderOperation(operation.target, localFailure(error)); }
      finally { setBusy(form, operation.target, false); }
    });
  });

  const restore = documentRef.getElementById('restore');
  const restoreOperation = bindOperation(documentRef, restore, operationId('restore', 0), () => post('/api/restore', Object.fromEntries(new FormData(restore)), token), [...restore.elements], result => writeOutput(documentRef, 'backup-results', result));
  restore.addEventListener('submit', async event => { event.preventDefault(); if (!confirm('Restore will replace the target database. Continue?')) return; await restoreOperation.run(); });

  const backup = documentRef.getElementById('backup-create');
  const backupOperation = bindOperation(documentRef, backup, operationId('backup', 0), () => post('/api/backup', {}, token), [backup], result => writeOutput(documentRef, 'backup-results', result));
  backup.addEventListener('click', backupOperation.run);

  for (const [id, operation] of [['cluster-check', 'check'], ['cluster-dry', 'rebuild'], ['cluster-rebuild', 'rebuild']]) {
    const button = documentRef.getElementById(id);
    const clusterOperation = bindOperation(documentRef, button, operationId(id, 0), () => post('/api/rpc', { method: 'cluster_maintenance', params: { room: 'tuner', operation, ...(id === 'cluster-dry' ? { dryRun: true } : {}), ...(operation === 'rebuild' ? { confirm: 'REBUILD' } : {}) } }, token), [button], result => writeOutput(documentRef, 'cluster-results', result));
    button.addEventListener('click', async () => { if (id === 'cluster-rebuild' && !confirm('Rebuild cluster?')) return; await clusterOperation.run(); });
  }
}

if (typeof document !== 'undefined') initializeGui();

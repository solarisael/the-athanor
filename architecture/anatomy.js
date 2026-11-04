import cytoscape from 'cytoscape';
import fcose from 'cytoscape-fcose';

cytoscape.use(fcose);

const $ = selector => document.querySelector(selector);
const reducedMotion = matchMedia('(prefers-reduced-motion: reduce)');

try {
  const response = await fetch('anatomy.jsonl');
  if (!response.ok) throw new Error(`Map request failed: ${response.status}`);
  start((await response.text()).trim().split('\n').map(JSON.parse));
} catch (error) {
  $('#counts').textContent = 'The graph could not load.';
  $('#detail').textContent = error.message;
  console.error(error);
}

function element(tag, text, className) {
  const result = document.createElement(tag);
  if (text !== undefined) result.textContent = text;
  if (className) result.className = className;
  return result;
}

function sourceList(references) {
  const box = element('div');
  for (const source of references) {
    box.append(element('div', `${source.path}:${source.line}–${source.endLine}`, 'source'));
  }
  return box;
}

function evidence(record) {
  const detail = element('details');
  detail.append(element('summary', 'Connection and source'), element('p', record.description));
  if (record.condition) detail.append(element('p', 'Condition: ' + record.condition));
  for (const text of record.details || []) detail.append(element('p', text));
  detail.append(sourceList(record.sources));
  return detail;
}

function start(records) {
  const meta = records.find(record => record.type === 'meta');
  const nodes = records.filter(record => record.type === 'node');
  const edges = records.filter(record => record.type === 'edge');
  const byId = new Map(nodes.map(node => [node.id, node]));
  const edgeById = new Map(edges.map(edge => [edge.id, edge]));
  const adjacency = new Map(nodes.map(node => [node.id, []]));
  for (const edge of edges) {
    adjacency.get(edge.from).push(edge);
    if (edge.to !== edge.from) adjacency.get(edge.to).push(edge);
  }
  let selected = null;
  let mode = 'all';
  let layout = null;
  let initialLayout = true;
  let wholePositions = new Map();

  $('#counts').textContent = `${nodes.length} nodes · ${edges.length} connections · ${meta.capturedAt.slice(0, 10)}`;
  for (const kind of [...new Set(nodes.map(node => node.kind))].sort()) {
    const option = element('option', kind);
    option.value = kind;
    $('#kind').append(option);
  }

  const cy = cytoscape({
    container: $('#graph'),
    elements: [
      ...nodes.map((node, index) => ({
        data: {
          id: node.id, label: node.label, kind: node.kind,
          cardLabel: `${node.label.length > 42 ? node.label.slice(0, 40) + '…' : node.label}\n${node.kind}`,
        },
        position: { x: index % 24 * 300, y: Math.floor(index / 24) * 160 },
      })),
      ...edges.map(edge => ({ data: { id: edge.id, source: edge.from, target: edge.to, label: edge.relation } })),
    ],
    layout: { name: 'preset' },
    minZoom: 0.025,
    maxZoom: 4,
    boxSelectionEnabled: false,
    style: [
      { selector: 'node', style: {
        shape: 'round-rectangle', 'background-color': '#192330',
        'border-color': '#84b9df', 'border-width': 1.5, width: 200, height: 76,
        label: 'data(cardLabel)', color: '#e6edf6', 'font-family': 'system-ui', 'font-size': 13,
        'text-valign': 'center', 'text-halign': 'center',
        'min-zoomed-font-size': 8, 'text-wrap': 'wrap', 'text-max-width': 180,
        'overlay-opacity': 0,
      } },
      { selector: 'node[kind="organ"], node[kind="system"]', style: {
        'background-color': '#2b2721', 'border-color': '#e8bd77',
        'border-width': 2, width: 220, height: 84, 'font-size': 14, 'font-weight': 600,
      } },
      { selector: 'node[kind="store"]', style: { 'border-color': '#99ccac' } },
      { selector: 'node[kind="external"]', style: { 'border-color': '#c5acf0' } },
      { selector: 'node[kind="tool"], node[kind="command"]', style: { 'border-color': '#e49b90' } },
      { selector: 'edge', style: {
        width: 1, 'line-color': '#53677e', 'target-arrow-color': '#53677e',
        'target-arrow-shape': 'triangle', 'arrow-scale': .65, 'curve-style': 'bezier', opacity: .42,
        'overlay-opacity': 0,
      } },
      { selector: '.dim', style: { opacity: .10 } },
      { selector: 'node.neighbor', style: { 'border-width': 2, 'border-color': '#e6edf6', 'min-zoomed-font-size': 0 } },
      { selector: 'node.focus', style: { 'border-width': 4, 'border-color': '#fff3d7', 'background-color': '#353028', 'min-zoomed-font-size': 0 } },
      { selector: 'edge.focus', style: {
        width: 2, opacity: .95, 'line-color': '#e8bd77', 'target-arrow-color': '#e8bd77',
        label: 'data(label)', 'font-size': 11, color: '#e6edf6', 'min-zoomed-font-size': 11,
        'text-background-color': '#10151c', 'text-background-opacity': .95, 'text-background-padding': 3,
        'text-rotation': 'autorotate',
      } },
      { selector: '.hidden', style: { display: 'none' } },
    ],
  });

  function catalogue() {
    const query = $('#search').value.toLowerCase().trim();
    const kind = $('#kind').value;
    const list = $('#nodes');
    list.replaceChildren();
    const matches = nodes.filter(node => (!kind || node.kind === kind) && (!query || `${node.id} ${node.label} ${node.description}`.toLowerCase().includes(query)));
    matches.sort((a, b) => (a.kind === 'organ' ? 0 : 1) - (b.kind === 'organ' ? 0 : 1) || a.label.localeCompare(b.label));
    $('#match-count').textContent = `${matches.length} matching nodes`;
    for (const node of matches) {
      const button = element('button', node.label, 'node-choice');
      button.dataset.id = node.id;
      button.setAttribute('aria-current', String(node.id === selected));
      button.append(element('small', node.kind));
      button.onclick = () => choose(node.id, true);
      list.append(button);
    }
    if (!matches.length) list.append(element('p', 'No matching nodes.', 'muted'));
  }

  function inspectNode(id) {
    const node = byId.get(id);
    const panel = $('#detail');
    panel.replaceChildren(element('h2', node.label), element('code', node.id), element('p', node.description));
    const pointers = element('details');
    pointers.append(element('summary', 'Node source pointers'), sourceList(node.sources));
    panel.append(pointers);
    const links = adjacency.get(id);
    panel.append(element('h3', `${links.length} incoming and outgoing connections`));
    for (const edge of links) {
      const outward = edge.from === id;
      const other = byId.get(outward ? edge.to : edge.from);
      const button = element('button', `${outward ? '→' : '←'} ${edge.relation} · ${other.label}`, 'connection');
      button.onclick = () => choose(other.id, true);
      panel.append(button, evidence(edge));
    }
    panel.scrollTop = 0;
  }

  function highlight() {
    cy.batch(() => {
      cy.elements().removeClass('focus neighbor dim');
      if (!selected) return;
      const node = cy.getElementById(selected);
      const neighborhood = node.closedNeighborhood();
      cy.elements().not(neighborhood).addClass('dim');
      neighborhood.nodes().not(node).addClass('neighbor');
      neighborhood.edges().addClass('focus');
      node.addClass('focus');
    });
  }

  function status() {
    $('#status').textContent = `${cy.nodes().not('.hidden').length} visible nodes · ${Math.round(cy.zoom() * 100)}% zoom`;
  }

  function show(nextMode) {
    cy.stop();
    if (mode === 'all') wholePositions = new Map(cy.nodes().map(node => [node.id(), { ...node.position() }]));
    if (mode !== 'all') {
      cy.nodes().positions(node => wholePositions.get(node.id()) || node.position());
    }
    mode = nextMode;
    const allowed = new Set(nodes.filter(node => mode === 'all' || node.kind === 'organ' || node.kind === 'system').map(node => node.id));
    if (mode === 'neighbors') {
      allowed.clear();
      const id = selected || 'system:athanor';
      allowed.add(id);
      for (const edge of adjacency.get(id)) { allowed.add(edge.from); allowed.add(edge.to); }
    }
    cy.batch(() => {
      cy.nodes().forEach(node => node.toggleClass('hidden', !allowed.has(node.id())));
      cy.edges().forEach(edge => edge.toggleClass('hidden', !allowed.has(edge.source().id()) || !allowed.has(edge.target().id())));
    });
    for (const name of ['all', 'organs', 'neighbors']) $(`#${name}`).setAttribute('aria-pressed', String(mode === name));
    highlight();
    if (mode === 'organs') {
      cy.elements().not('.hidden').layout({
        name: 'grid', cols: 4, spacingFactor: 1.15,
        avoidOverlap: true, nodeDimensionsIncludeLabels: true,
      }).run();
    } else if (mode === 'neighbors') {
      cy.elements().not('.hidden').layout({
        name: 'concentric', avoidOverlap: true, minNodeSpacing: 45,
        nodeDimensionsIncludeLabels: true,
        concentric: node => node.id() === (selected || 'system:athanor') ? 2 : 1,
        levelWidth: () => 1,
      }).run();
    }
    cy.fit(cy.elements().not('.hidden'), 60);
    status();
  }

  function choose(id, center = false) {
    selected = id;
    if (mode === 'neighbors') show('neighbors');
    else if (cy.getElementById(id).hasClass('hidden')) show('all');
    inspectNode(id);
    highlight();
    for (const button of document.querySelectorAll('.node-choice')) button.setAttribute('aria-current', String(button.dataset.id === id));
    if (center) {
      cy.stop();
      const target = { center: { eles: cy.getElementById(id) }, zoom: Math.max(.8, cy.zoom()) };
      if (reducedMotion.matches) { cy.center(target.center.eles); cy.zoom(target.zoom); cy.center(target.center.eles); }
      else cy.animate(target, { duration: 250 });
    }
  }

  function spread() {
    if (mode !== 'all') {
      show(mode);
      return;
    }
    if (layout) layout.stop();
    $('#layout').disabled = true;
    $('#layout').textContent = 'Arranging…';
    $('#status').textContent = 'Separating the graph. Navigation returns when the layout settles.';
    requestAnimationFrame(() => {
      layout = cy.elements().not('.hidden').layout({
        name: 'fcose', quality: 'proof', animate: false, randomize: true,
        fit: true, padding: 65, nodeDimensionsIncludeLabels: false,
        packComponents: false, nodeSeparation: 300,
        idealEdgeLength: () => 500, nodeRepulsion: () => 10000000,
        edgeElasticity: () => 0.04, gravity: 0.05, numIter: 5000,
        tilingPaddingVertical: 50, tilingPaddingHorizontal: 50,
        stop() {
          $('#layout').disabled = false;
          $('#layout').textContent = 'Spread out';
          status();
          if (initialLayout) {
            initialLayout = false;
            show('organs');
          }
        },
      });
      layout.run();
    });
  }

  cy.on('tap', 'node', event => choose(event.target.id()));
  cy.on('tap', 'edge', event => {
    const edge = edgeById.get(event.target.id());
    const panel = $('#detail');
    panel.replaceChildren(element('h2', edge.relation), element('p', `${byId.get(edge.from).label} → ${byId.get(edge.to).label}`), evidence(edge));
    panel.querySelector('details').open = true;
    panel.scrollTop = 0;
  });
  cy.on('tap', event => { if (event.target === cy) { selected = null; highlight(); } });
  cy.on('zoom', status);
  $('#search').oninput = catalogue;
  $('#kind').onchange = catalogue;
  $('#all').onclick = () => { selected = null; show('all'); };
  $('#organs').onclick = () => { selected = null; show('organs'); };
  $('#neighbors').onclick = () => show('neighbors');
  $('#fit').onclick = () => { cy.stop(); cy.fit(cy.elements().not('.hidden'), 60); };
  $('#layout').onclick = spread;
  for (const [name, factor] of [['plus', 1.3], ['minus', 1 / 1.3]]) {
    $(`#${name}`).onclick = () => cy.zoom({ level: cy.zoom() * factor, renderedPosition: { x: cy.width() / 2, y: cy.height() / 2 } });
  }
  $('#graph').onkeydown = event => {
    const movements = { ArrowLeft: { x: 60, y: 0 }, ArrowRight: { x: -60, y: 0 }, ArrowUp: { x: 0, y: 60 }, ArrowDown: { x: 0, y: -60 } };
    if (movements[event.key]) { event.preventDefault(); cy.panBy(movements[event.key]); }
  };
  new ResizeObserver(() => cy.resize()).observe($('#graph'));
  catalogue();
  spread();
}

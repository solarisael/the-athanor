import cytoscape from 'cytoscape';
import fcose from 'cytoscape-fcose';

cytoscape.use(fcose);

const $ = selector => document.querySelector(selector);
const reducedMotion = matchMedia('(prefers-reduced-motion: reduce)');

// A part can belong to two lenses at once: the organ it serves and the crate that holds it.
// Each card gets one drawer, so the most specific owner wins in this order.
const CABINET_OWNER_ORDER = ['organ', 'module', 'component', 'package'];

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

// `contains` is structural membership, and an organ's `implemented_by` names the code it owns.
// Both make drawers. The system node stays a card, so the organs never vanish into one box.
function cabinetTree(edges, byId) {
  const owners = new Map();

  for (const edge of edges) {
    const ownerKind = byId.get(edge.from).kind;
    const membership = edge.relation === 'contains' || (edge.relation === 'implemented_by' && ownerKind === 'organ');

    if (!membership || !CABINET_OWNER_ORDER.includes(ownerKind) || edge.from === edge.to) continue;
    if (!owners.has(edge.to)) owners.set(edge.to, []);
    owners.get(edge.to).push(edge.from);
  }

  const parent = new Map();
  const rank = id => CABINET_OWNER_ORDER.indexOf(byId.get(id).kind);
  for (const [child, candidates] of owners) {
    parent.set(child, candidates.sort((a, b) => rank(a) - rank(b))[0]);
  }

  // Two lenses can point at each other. A drawer inside itself has no top, so the loop is cut there.
  for (const child of [...parent.keys()]) {
    const seen = new Set([child]);
    for (let current = parent.get(child); current; current = parent.get(current)) {
      if (seen.has(current)) {
        parent.delete(child);
        break;
      }
      seen.add(current);
    }
  }

  const children = new Map();
  for (const [child, owner] of parent) {
    if (!children.has(owner)) children.set(owner, []);
    children.get(owner).push(child);
  }

  const partCount = new Map();
  const countParts = id => {
    if (partCount.has(id)) return partCount.get(id);
    const total = (children.get(id) || []).reduce((sum, child) => sum + 1 + countParts(child), 0);
    partCount.set(id, total);
    return total;
  };
  for (const owner of children.keys()) countParts(owner);

  return { parent, children, partCount };
}

function start(records) {
  const meta = records.find(record => record.type === 'meta');
  const nodes = records.filter(record => record.type === 'node');
  const edges = records.filter(record => record.type === 'edge');
  const byId = new Map(nodes.map(node => [node.id, node]));
  const { parent, children, partCount } = cabinetTree(edges, byId);
  const open = new Set();
  const positions = new Map();
  let foldedEdges = new Map();
  let selected = null;
  let mode = 'organs';
  let depth = 1;
  let layout = null;
  let wholeArranged = false;

  $('#counts').textContent = `${nodes.length} nodes · ${edges.length} connections · ${children.size} cabinets · ${meta.capturedAt.slice(0, 10)}`;
  for (const kind of [...new Set(nodes.map(node => node.kind))].sort()) {
    const option = element('option', kind);
    option.value = kind;
    $('#kind').append(option);
  }

  const cy = cytoscape({
    container: $('#graph'),
    elements: [],
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
      { selector: 'node.cabinet', style: { 'border-style': 'double', 'border-width': 5 } },
      { selector: ':parent', style: {
        'background-opacity': .28, 'border-style': 'dashed', 'border-width': 1.5,
        'text-valign': 'top', 'text-halign': 'center', 'text-margin-y': -8, padding: 28,
        'min-zoomed-font-size': 0,
      } },
      { selector: 'edge', style: {
        width: 'mapData(count, 1, 12, 1, 4)', 'line-color': '#53677e', 'target-arrow-color': '#53677e',
        'target-arrow-shape': 'triangle', 'arrow-scale': .65, 'curve-style': 'bezier', opacity: .42,
        'overlay-opacity': 0,
      } },
      { selector: '.dim', style: { opacity: .10 } },
      { selector: 'node.neighbor', style: { 'border-width': 2, 'border-color': '#e6edf6', 'min-zoomed-font-size': 0 } },
      { selector: 'node.focus', style: { 'border-width': 4, 'border-color': '#fff3d7', 'background-color': '#353028', 'min-zoomed-font-size': 0 } },
      { selector: 'edge.focus', style: {
        opacity: .95, 'line-color': '#e8bd77', 'target-arrow-color': '#e8bd77',
        label: 'data(label)', 'font-size': 11, color: '#e6edf6', 'min-zoomed-font-size': 11,
        'text-background-color': '#10151c', 'text-background-opacity': .95, 'text-background-padding': 3,
        'text-rotation': 'autorotate',
      } },
    ],
  });

  // The card that stands for a part: the part itself, or the outermost closed drawer around it.
  function shownAs(id) {
    let shown = id;
    for (let current = parent.get(id); current; current = parent.get(current)) {
      if (!open.has(current)) shown = current;
    }
    return shown;
  }

  function topOwner(id) {
    let top = id;
    for (let current = parent.get(id); current; current = parent.get(current)) top = current;
    return top;
  }

  // Connections between parts inside one closed drawer disappear into it. Connections that leave a
  // drawer are drawn from the drawer and grouped by relation, so a closed organ still shows its reach.
  function foldEdges() {
    const folded = new Map();

    for (const edge of edges) {
      if (parent.get(edge.to) === edge.from) continue;

      const from = shownAs(edge.from);
      const to = shownAs(edge.to);

      if (from === to) continue;

      const key = `${from}\u0000${edge.relation}\u0000${to}`;
      if (!folded.has(key)) folded.set(key, { id: `fold:${key}`, from, to, relation: edge.relation, members: [] });
      folded.get(key).members.push(edge);
    }

    return folded;
  }

  function withOpenOwners(visible) {
    for (const id of [...visible]) {
      for (let current = parent.get(id); current; current = parent.get(current)) {
        if (open.has(current)) visible.add(current);
      }
    }
    return visible;
  }

  function reachable(folded) {
    const start = selected ? shownAs(selected) : 'system:athanor';
    const distance = new Map([[start, 0]]);
    for (const id of nodes.map(node => node.id)) {
      if (id !== start && shownAs(id) === id && isInside(id, start)) distance.set(id, 0);
    }

    const links = new Map();
    for (const edge of folded.values()) {
      if (!links.has(edge.from)) links.set(edge.from, []);
      if (!links.has(edge.to)) links.set(edge.to, []);
      links.get(edge.from).push(edge.to);
      links.get(edge.to).push(edge.from);
    }

    let frontier = [...distance.keys()];
    for (let hop = 1; hop <= depth && frontier.length; hop++) {
      const next = [];
      for (const id of frontier) {
        for (const other of links.get(id) || []) {
          if (distance.has(other)) continue;
          distance.set(other, hop);
          next.push(other);
        }
      }
      frontier = next;
    }

    return distance;
  }

  function isInside(id, owner) {
    for (let current = parent.get(id); current; current = parent.get(current)) {
      if (current === owner) return true;
    }
    return false;
  }

  function cardLabel(node) {
    const label = node.label.length > 42 ? node.label.slice(0, 40) + '…' : node.label;
    if (!children.has(node.id)) return `${label}\n${node.kind}`;
    if (open.has(node.id)) return `▾ ${label} · ${node.kind}`;
    return `${label}\n${node.kind} · ▸ ${partCount.get(node.id)} parts`;
  }

  function render() {
    for (const node of cy.nodes()) positions.set(node.id(), { ...node.position() });

    foldedEdges = foldEdges();
    const shown = new Set(nodes.map(node => shownAs(node.id)));
    let visible;
    let distance = null;

    if (mode === 'organs') {
      visible = new Set([...shown].filter(id => byId.get(topOwner(id)).kind === 'organ'));
    } else if (mode === 'neighbors') {
      distance = reachable(foldedEdges);
      visible = new Set(distance.keys());
    } else {
      visible = new Set(shown);
    }
    visible = withOpenOwners(visible);

    const cards = [...visible].map(id => {
      const node = byId.get(id);
      const owner = parent.get(id);
      return {
        data: {
          id, label: node.label, kind: node.kind, cardLabel: cardLabel(node),
          distance: distance?.get(id) ?? 0,
          ...(owner && open.has(owner) && visible.has(owner) ? { parent: owner } : {}),
        },
        classes: children.has(id) && !open.has(id) ? 'cabinet' : '',
        position: positions.get(id) || { x: 0, y: 0 },
      };
    });
    const lines = [...foldedEdges.values()]
      .filter(edge => visible.has(edge.from) && visible.has(edge.to))
      .map(edge => ({
        data: {
          id: edge.id, source: edge.from, target: edge.to, count: edge.members.length,
          label: edge.members.length > 1 ? `${edge.relation} ×${edge.members.length}` : edge.relation,
        },
      }));

    cy.batch(() => {
      cy.elements().remove();
      cy.add([...cards, ...lines]);
    });
    arrange();
    highlight();
    status();
  }
  function arrange() {
    if (layout) layout.stop();

    const compound = cy.nodes(':parent').nonempty();
    let options;

    if (mode === 'organs' && !compound) {
      options = { name: 'grid', cols: 4, spacingFactor: 1.15, avoidOverlap: true, nodeDimensionsIncludeLabels: true };
    } else if (mode === 'neighbors' && !compound && depth === 1) {
      // One hop reads as a ring. Two or more hops pass through hubs and need the force layout.
      options = {
        name: 'concentric', avoidOverlap: true, minNodeSpacing: 45, nodeDimensionsIncludeLabels: true,
        concentric: node => depth + 1 - node.data('distance'), levelWidth: () => 1,
      };
    } else if (mode === 'all' && wholeArranged && cy.nodes().every(node => positions.has(node.id()))) {
      cy.fit(cy.elements(), 60);
      return;
    } else {
      // Only the force layout knows how to size a drawer around the cards inside it.
      options = {
        name: 'fcose', quality: mode === 'all' ? 'proof' : 'default', animate: false, randomize: true,
        fit: true, padding: 60, nodeDimensionsIncludeLabels: false, packComponents: false,
        nodeSeparation: 220, idealEdgeLength: () => (mode === 'all' ? 500 : 260),
        nodeRepulsion: () => (mode === 'all' ? 10000000 : 400000),
        edgeElasticity: () => 0.04, gravity: 0.05, numIter: mode === 'all' ? 5000 : 2500,
        tilingPaddingVertical: 50, tilingPaddingHorizontal: 50,
      };
      if (mode === 'all') wholeArranged = true;
    }

    layout = cy.elements().layout(options);
    layout.run();
    cy.fit(cy.elements(), 60);
  }

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
      button.append(element('small', children.has(node.id) ? `${node.kind} · ${partCount.get(node.id)} parts` : node.kind));
      button.onclick = () => choose(node.id, true);
      list.append(button);
    }
    if (!matches.length) list.append(element('p', 'No matching nodes.', 'muted'));
  }

  function inspectNode(id) {
    const node = byId.get(id);
    const panel = $('#detail');
    panel.replaceChildren(element('h2', node.label), element('code', node.id), element('p', node.description));

    if (children.has(id)) {
      const toggle = element('button', open.has(id) ? 'Close cabinet' : `Open cabinet · ${partCount.get(id)} parts`, 'connection');
      toggle.onclick = () => toggleCabinet(id);
      panel.append(toggle);
    }

    const owner = parent.get(id);
    if (owner) {
      const up = element('button', `↑ inside · ${byId.get(owner).label}`, 'connection');
      up.onclick = () => choose(owner, true);
      panel.append(up);
    }

    const pointers = element('details');
    pointers.append(element('summary', 'Node source pointers'), sourceList(node.sources));
    panel.append(pointers);

    const shownId = shownAs(id);
    const links = [...foldedEdges.values()].filter(edge => edge.from === shownId || edge.to === shownId);
    panel.append(element('h3', `${links.length} connections from this card`));
    for (const edge of links) {
      const outward = edge.from === shownId;
      const other = byId.get(outward ? edge.to : edge.from);
      const count = edge.members.length > 1 ? ` ×${edge.members.length}` : '';
      const button = element('button', `${outward ? '→' : '←'} ${edge.relation}${count} · ${other.label}`, 'connection');
      button.onclick = () => choose(other.id, true);
      panel.append(button);
      for (const member of edge.members) panel.append(evidence(member));
    }
    panel.scrollTop = 0;
  }

  function inspectEdge(id) {
    const edge = foldedEdges.get(id.slice('fold:'.length));
    const panel = $('#detail');
    const count = edge.members.length > 1 ? ` ×${edge.members.length}` : '';
    panel.replaceChildren(element('h2', `${edge.relation}${count}`), element('p', `${byId.get(edge.from).label} → ${byId.get(edge.to).label}`));
    for (const member of edge.members) {
      const record = evidence(member);
      record.querySelector('summary').textContent = `${byId.get(member.from).label} → ${byId.get(member.to).label}`;
      panel.append(record);
    }
    if (edge.members.length === 1) panel.querySelector('details').open = true;
    panel.scrollTop = 0;
  }

  function highlight() {
    cy.batch(() => {
      cy.elements().removeClass('focus neighbor dim');
      if (!selected) return;
      const node = cy.getElementById(shownAs(selected));
      if (node.empty()) return;
      const neighborhood = node.closedNeighborhood().union(node.descendants());
      cy.elements().not(neighborhood).not(node.ancestors()).addClass('dim');
      neighborhood.nodes().not(node).addClass('neighbor');
      neighborhood.edges().addClass('focus');
      node.addClass('focus');
    });
  }

  function status() {
    const sieve = mode === 'neighbors' ? ` · depth ${depth}` : '';
    $('#status').textContent = `${cy.nodes().length} visible cards · ${open.size} open cabinets${sieve} · ${Math.round(cy.zoom() * 100)}% zoom`;
  }

  function show(nextMode) {
    cy.stop();
    mode = nextMode;
    for (const name of ['all', 'organs', 'neighbors']) $(`#${name}`).setAttribute('aria-pressed', String(mode === name));
    render();
  }

  function toggleCabinet(id) {
    if (open.has(id)) {
      open.delete(id);
      for (const child of children.get(id)) closeInside(child);
    } else {
      open.add(id);
      for (let current = parent.get(id); current; current = parent.get(current)) open.add(current);
    }
    render();
    if (selected) inspectNode(selected);
  }

  function closeInside(id) {
    open.delete(id);
    for (const child of children.get(id) || []) closeInside(child);
  }

  function choose(id, center = false) {
    selected = id;
    if (mode === 'neighbors') render();
    else if (cy.getElementById(shownAs(id)).empty()) show('all');
    inspectNode(id);
    highlight();
    for (const button of document.querySelectorAll('.node-choice')) button.setAttribute('aria-current', String(button.dataset.id === id));
    if (center) {
      cy.stop();
      const target = { center: { eles: cy.getElementById(shownAs(id)) }, zoom: Math.max(.8, cy.zoom()) };
      if (reducedMotion.matches) { cy.center(target.center.eles); cy.zoom(target.zoom); cy.center(target.center.eles); }
      else cy.animate(target, { duration: 250 });
    }
  }

  function spread() {
    $('#layout').disabled = true;
    $('#layout').textContent = 'Arranging…';
    $('#status').textContent = 'Arranging the cards. Navigation returns when the layout settles.';
    requestAnimationFrame(() => {
      if (mode === 'all') wholeArranged = false;
      arrange();
      $('#layout').disabled = false;
      $('#layout').textContent = 'Spread out';
      status();
    });
  }

  cy.on('tap', 'node', event => choose(event.target.id()));
  cy.on('dbltap', 'node', event => {
    if (children.has(event.target.id())) toggleCabinet(event.target.id());
  });
  cy.on('tap', 'edge', event => inspectEdge(event.target.id()));
  cy.on('tap', event => { if (event.target === cy) { selected = null; highlight(); } });
  cy.on('zoom', status);
  $('#search').oninput = catalogue;
  $('#kind').onchange = catalogue;
  $('#all').onclick = () => { selected = null; show('all'); };
  $('#organs').onclick = () => { selected = null; show('organs'); };
  $('#neighbors').onclick = () => show('neighbors');
  $('#depth').oninput = () => {
    depth = Number($('#depth').value);
    $('#depth-value').textContent = String(depth);
    show('neighbors');
  };
  $('#close-all').onclick = () => { open.clear(); render(); };
  $('#fit').onclick = () => { cy.stop(); cy.fit(cy.elements(), 60); };
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
  show('organs');
}

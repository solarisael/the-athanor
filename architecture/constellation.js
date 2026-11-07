// Constellation: organs are bubbles, their parts are stars inside them.
// A bubble's size on screen decides how much of its inside you can read, so zoom is the sieve.
// The camera moves only on a user action: wheel, drag, a button, a key, a search, or a panel link.
// anatomy.overlay.jsonl adds what the map lacks: later organs, state machines, and defects.
// anatomy.stale.json marks the stars whose cited source changed after the map snapshot.

const MEMBERSHIP = new Set(['contains', 'implemented_by']);
const KIND_COLORS = {
  module: '#84b9df', store: '#99ccac', external: '#c5acf0', tool: '#e49b90',
  command: '#e49b90', operation: '#b9a7e8', file: '#d6c48f', endpoint: '#e0a8c8',
};
const SEVERITY = {
  high: { rank: 3, color: '#ef6f6c' },
  med: { rank: 2, color: '#e8b04f' },
  low: { rank: 1, color: '#8fa3b8' },
};

// Screen radius thresholds. Below DOTS a bubble shows its part count; from CARDS up its parts carry names.
const DOTS_RADIUS_PX = 70;
const CARDS_RADIUS_PX = 200;
const RIM_GRAB_PX = 10;
const GRIP_RADIUS_PX = 11;
const BADGE_RADIUS_PX = 13;
const MIN_BUBBLE_RADIUS = 60;
const SAVE_DELAY_MS = 400;
const FLY_MS = 380;
const ZOOM_STEP = 1.3;
const ROW_WIDTH = 3600;
const BUBBLE_GAP = 180;
// The labels below a bubble keep their screen size, so rows need more room than columns.
const ROW_GAP = 320;
// The smallest empty space between two organ rims. Arrange and Tidy up both keep at least this much.
const ORGAN_GAP = 300;
const SEARCH_LIMIT = 8;

const $ = selector => document.querySelector(selector);
const reducedMotion = matchMedia('(prefers-reduced-motion: reduce)');

async function start() {
  const [records, overlay, saved, stale] = await Promise.all([
    fetchLines('anatomy.jsonl', true),
    fetchLines('anatomy.overlay.jsonl', false),
    fetchJson('anatomy.layout.json'),
    fetchJson('anatomy.stale.json'),
  ]);

  const meta = records.find(record => record.type === 'meta');
  const nodes = records.filter(record => record.type === 'node');
  const edges = records.filter(record => record.type === 'edge');
  const overlayOrgans = overlay.filter(record => record.type === 'organ');
  const machines = overlay.filter(record => record.type === 'machine');
  const findings = overlay.filter(record => record.type === 'finding');
  const tags = overlay.filter(record => record.type === 'tag');
  const crates = overlay.filter(record => record.type === 'crate');

  const byId = new Map(nodes.map(node => [node.id, node]));
  for (const organ of overlayOrgans) {
    byId.set(organ.id, { ...organ, kind: 'organ', sources: organ.sources || [], overlay: true });
  }

  // A crate organ reaches its modules through the package. Domain organs reach theirs in one step,
  // so they keep the modules they name and the crate organ takes the rest.
  const memberEdges = [
    ...overlayOrgans.flatMap(organ =>
      organ.members.filter(member => byId.has(member)).map(member => ({ from: organ.id, to: member, relation: 'implemented_by' }))),
    ...crates.filter(crate => byId.has(crate.organ) && byId.has(crate.package))
      .map(crate => ({ from: crate.organ, to: crate.package, relation: 'implemented_by' })),
  ];
  // Order decides membership ties: domain organs first, crate organs last.
  const crateOrgans = new Set(crates.map(crate => crate.organ));
  const bubbleIds = [
    ...overlayOrgans.map(organ => organ.id),
    ...nodes.filter(node => node.kind === 'organ').map(node => node.id).sort(),
  ].sort((a, b) => Number(crateOrgans.has(a)) - Number(crateOrgans.has(b)));
  const ownerOf = claimMembers([...memberEdges, ...edges], byId, bubbleIds);
  const bubbles = placeBubbles(bubbleIds, ownerOf, byId, saved?.bubbles || {});
  const links = edges.filter(edge =>
    !MEMBERSHIP.has(edge.relation) && edge.from !== edge.to && ownerOf.has(edge.from) && ownerOf.has(edge.to));

  const stalePaths = new Set(Object.keys(stale?.files || {}));
  const staleNodes = new Set(nodes.filter(node => node.sources.some(source => stalePaths.has(source.path))).map(node => node.id));

  const world = {
    byId, bubbles, ownerOf, links,
    bubbleById: new Map(bubbles.map(bubble => [bubble.id, bubble])),
    crossings: groupCrossings(links, ownerOf),
    machinesByNode: indexByNode(machines),
    findingsByNode: indexByNode(findings),
    machineById: new Map(machines.map(machine => [machine.id, machine])),
    tagById: new Map(tags.map(tag => [tag.id, tag])),
    staleNodes,
    filter: { tag: '', severity: '' },
    highlight: null,
    selected: null,
  };

  const placedFindings = findings.filter(finding => finding.nodes.some(id => ownerOf.has(id))).length;
  $('#counts').textContent = [
    `${bubbles.length} organs`, `${ownerOf.size} parts`,
    `${placedFindings} of ${findings.length} problems on the map`, `map from ${meta.capturedAt.slice(0, 10)}`,
  ].join(' · ');

  let pending = false;
  world.redraw = () => {
    if (pending) return;

    pending = true;
    requestAnimationFrame(() => {
      pending = false;
      for (const view of world.views) view.draw();
    });
  };

  let saveTimer = null;
  world.scheduleSave = (note = '') => {
    clearTimeout(saveTimer);
    saveTimer = setTimeout(() => saveLayout(bubbles, note), SAVE_DELAY_MS);
  };
  world.select = id => select(world, id);
  world.goTo = id => goTo(world, id);

  world.views = [new Viewport($('#view-1'), world), new Viewport($('#view-2'), world)];
  world.activeView = world.views[0];
  world.views[0].resize();
  world.views[0].camera = world.views[0].cameraFor(bubbles);

  for (const view of world.views) view.listen();
  new ResizeObserver(() => {
    for (const view of world.views) view.resize();
    world.redraw();
  }).observe($('#views'));

  setupToolbar(world);
  setupSearch(world);
  setupFilters(world, findings);
  world.redraw();
}

async function fetchLines(path, required) {
  const response = await fetch(path);

  if (!response.ok && required) throw new Error(`${path} request failed: ${response.status}`);
  if (!response.ok) return [];

  const text = (await response.text()).trim();
  return text ? text.split('\n').map(line => JSON.parse(line)) : [];
}

async function fetchJson(path) {
  const response = await fetch(path);
  return response.ok ? response.json() : null;
}

function indexByNode(records) {
  const index = new Map();
  for (const record of records) {
    for (const id of record.nodes) {
      if (!index.has(id)) index.set(id, []);
      index.get(id).push(record);
    }
  }
  return index;
}

// Each part joins the bubble that reaches it in the fewest membership steps. On a tie the earlier
// bubble wins, and the overlay organs come first, so chat.ts sits in Chat and not in its crate.
// A store with no membership joins the bubble whose parts read or write it most often.
function claimMembers(edges, byId, bubbleIds) {
  const children = new Map();
  for (const edge of edges) {
    const ownerKind = byId.get(edge.from)?.kind;
    const membership = edge.relation === 'contains' || (edge.relation === 'implemented_by' && ownerKind === 'organ');

    if (!membership || !byId.has(edge.to)) continue;
    if (!children.has(edge.from)) children.set(edge.from, []);
    children.get(edge.from).push(edge.to);
  }

  const bubbleSet = new Set(bubbleIds);
  const owner = new Map();
  for (const bubble of bubbleIds) {
    const seen = new Set([bubble]);
    const queue = [[bubble, 0]];
    while (queue.length) {
      const [id, distance] = queue.shift();
      for (const child of children.get(id) || []) {
        if (seen.has(child) || bubbleSet.has(child)) continue;

        seen.add(child);
        const claim = owner.get(child);
        // A package is a route to its modules, never a star of its own.
        const isPackage = byId.get(child).kind === 'package';
        if (!isPackage && (!claim || distance + 1 < claim.distance)) owner.set(child, { bubble, distance: distance + 1 });
        queue.push([child, distance + 1]);
      }
    }
  }

  const touches = new Map();
  for (const edge of edges) {
    const target = byId.get(edge.to);
    const reachesStore = (edge.relation === 'reads' || edge.relation === 'writes') && target?.kind === 'store';

    if (!reachesStore || !owner.has(edge.from) || owner.has(edge.to)) continue;
    if (!touches.has(edge.to)) touches.set(edge.to, new Map());
    const counts = touches.get(edge.to);
    const bubble = owner.get(edge.from).bubble;
    counts.set(bubble, (counts.get(bubble) || 0) + 1);
  }
  for (const [store, counts] of touches) {
    const [bubble] = [...counts].sort((a, b) => b[1] - a[1])[0];
    owner.set(store, { bubble, distance: Infinity });
  }

  return new Map([...owner].map(([id, claim]) => [id, claim.bubble]));
}

// The first arrangement fills rows left to right. After that the layout file owns every position.
// An organ that the saved layout does not know yet starts in a row below it, so it lands in free space.
function placeBubbles(bubbleIds, ownerOf, byId, stored) {
  const bubbles = [];
  const known = Object.values(stored);
  const left = known.length ? Math.min(...known.map(bubble => bubble.x - bubble.r)) : 0;
  let x = left;
  let y = known.length ? Math.max(...known.map(bubble => bubble.y + bubble.r)) + ROW_GAP : 0;
  let rowHeight = 0;

  for (const id of bubbleIds) {
    const memberIds = [...ownerOf].filter(([, bubble]) => bubble === id).map(([member]) => member);
    memberIds.sort((a, b) => byId.get(a).kind.localeCompare(byId.get(b).kind) || byId.get(a).label.localeCompare(byId.get(b).label));

    const members = new Map();
    memberIds.forEach((member, order) => members.set(member, stored[id]?.members?.[member] ?? sunflower(order, memberIds.length)));
    const bubble = { id, hue: hueOf(id), label: byId.get(id).label, members };
    bubbles.push(bubble);

    if (stored[id]) {
      Object.assign(bubble, { x: stored[id].x, y: stored[id].y, r: stored[id].r });
      continue;
    }

    bubble.r = Math.max(110, 62 * Math.sqrt(memberIds.length));
    if (x > left && x + bubble.r * 2 > left + ROW_WIDTH) {
      x = left;
      y += rowHeight + ROW_GAP;
      rowHeight = 0;
    }

    bubble.x = x + bubble.r;
    bubble.y = y + bubble.r;
    x += bubble.r * 2 + BUBBLE_GAP;
    rowHeight = Math.max(rowHeight, bubble.r * 2);
  }

  return bubbles;
}

// The colour follows the organ's id, so adding an organ never repaints the others.
function hueOf(id) {
  let hash = 0;
  for (const char of id) hash = (hash * 31 + char.charCodeAt(0)) >>> 0;
  return hash % 360;
}

// Golden-angle spiral inside the unit disk: an even first arrangement with no layout pass.
function sunflower(order, count) {
  const angle = order * 2.39996;
  const radius = 0.8 * Math.sqrt((order + 0.5) / count);
  return [radius * Math.cos(angle), radius * Math.sin(angle)];
}

function groupCrossings(links, ownerOf) {
  const crossings = new Map();
  for (const link of links) {
    const from = ownerOf.get(link.from);
    const to = ownerOf.get(link.to);

    if (from === to) continue;

    const [a, b] = [from, to].sort();
    const key = `${a}\u0000${b}`;
    if (!crossings.has(key)) crossings.set(key, { a, b, count: 0 });
    crossings.get(key).count += 1;
  }
  return [...crossings.values()];
}

function worstSeverity(findings) {
  let worst = null;
  for (const finding of findings) {
    if (!worst || SEVERITY[finding.severity].rank > SEVERITY[worst].rank) worst = finding.severity;
  }
  return worst;
}

function sortedFindings(findings) {
  return [...findings].sort((a, b) => SEVERITY[b.severity].rank - SEVERITY[a.severity].rank);
}

// A part stays bright when it passes the problem filter and belongs to the highlighted state machine.
function isFaded(world, id) {
  if (world.highlight && !world.highlight.nodes.has(id)) return true;

  const { tag, severity } = world.filter;

  if (!tag && !severity) return false;

  return !(world.findingsByNode.get(id) || []).some(finding =>
    (!tag || finding.tags.includes(tag)) && (!severity || SEVERITY[finding.severity].rank >= SEVERITY[severity].rank));
}

function bubbleHealth(world, bubble) {
  const findings = new Set();
  let stale = 0;
  let hurt = 0;

  for (const id of bubble.members.keys()) {
    const own = world.findingsByNode.get(id) || [];
    if (own.length) hurt += 1;
    if (world.staleNodes.has(id)) stale += 1;
    for (const finding of own) findings.add(finding);
  }

  return { findings: sortedFindings(findings), stale, hurt };
}

function memberPoint(world, id) {
  const bubble = world.bubbleById.get(world.ownerOf.get(id));
  const [u, v] = bubble.members.get(id);
  return { x: bubble.x + u * bubble.r, y: bubble.y + v * bubble.r };
}

function ownerBubble(world, id) {
  return world.bubbleById.get(id) ?? world.bubbleById.get(world.ownerOf.get(id));
}

// A connection between bubbles leaves through the rim point that faces the other bubble.
function portPoint(bubble, other) {
  const dx = other.x - bubble.x;
  const dy = other.y - bubble.y;
  const length = Math.hypot(dx, dy) || 1;
  return { x: bubble.x + dx / length * bubble.r, y: bubble.y + dy / length * bubble.r };
}

async function saveLayout(bubbles, note) {
  const round = value => Math.round(value * 1000) / 1000;
  const layout = {
    version: 1,
    bubbles: Object.fromEntries(bubbles.map(bubble => [bubble.id, {
      x: Math.round(bubble.x),
      y: Math.round(bubble.y),
      r: Math.round(bubble.r),
      members: Object.fromEntries([...bubble.members].map(([id, [u, v]]) => [id, [round(u), round(v)]])),
    }])),
  };

  try {
    const response = await fetch('anatomy.layout.json', {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(layout, null, 2) + '\n',
    });
    $('#status').textContent = response.ok ? `${note} Layout saved.`.trim() : `Layout not saved: ${response.status} ${await response.text()}`;
  } catch (error) {
    $('#status').textContent = `Layout not saved: ${error.message}`;
  }
}

// ---- Navigation shared by the mouse, the keyboard, the search, and the panel ----

function select(world, id) {
  world.selected = id;
  world.redraw();
  renderPanel(world);
}

function goTo(world, id) {
  const view = world.activeView;
  const bubble = ownerBubble(world, id);

  if (!bubble) return;

  view.flyTo(view.cameraFor([bubble]));
  select(world, id);
}

// Pans just enough to bring a keyboard selection on screen, and opens a bubble that is too small to show it.
function reveal(world, view, id) {
  const bubble = ownerBubble(world, id);
  const isBubble = world.bubbleById.has(id);

  if (!isBubble && view.level(bubble) === 'count') {
    view.flyTo(view.cameraFor([bubble]), false);
    return;
  }

  const target = isBubble ? bubble : memberPoint(world, id);
  const point = view.toScreen(target);
  const margin = 90;
  const visible = point.x > margin && point.x < view.width - margin && point.y > margin && point.y < view.height - margin;

  if (visible) return;

  view.flyTo({ ...view.camera, x: target.x, y: target.y }, false);
}

// Arrow keys walk the organs, or the parts of the organ you are inside.
function moveSelection(world, view, dx, dy) {
  const selected = world.selected;
  const selectedBubble = selected && world.bubbleById.get(selected);
  const scope = selected && !selectedBubble
    ? ownerBubble(world, selected)
    : selectedBubble && view.level(selectedBubble) !== 'count' ? selectedBubble : null;
  const candidates = scope
    ? [...scope.members.keys()].map(id => ({ id, point: view.toScreen(memberPoint(world, id)) }))
    : world.bubbles.map(bubble => ({ id: bubble.id, point: view.toScreen(bubble) }));

  if (!selected) {
    const middle = { x: view.width / 2, y: view.height / 2 };
    const nearest = candidates.reduce((best, entry) =>
      Math.hypot(entry.point.x - middle.x, entry.point.y - middle.y) < Math.hypot(best.point.x - middle.x, best.point.y - middle.y) ? entry : best);
    world.select(nearest.id);
    reveal(world, view, nearest.id);
    return;
  }

  const from = selectedBubble ? view.toScreen(selectedBubble) : candidates.find(entry => entry.id === selected).point;
  const next = nearestInDirection(from, candidates.filter(entry => entry.id !== selected), dx, dy);

  if (!next) return;

  world.select(next.id);
  reveal(world, view, next.id);
}

function nearestInDirection(from, candidates, dx, dy) {
  let best = null;
  let bestScore = Infinity;

  for (const entry of candidates) {
    const vx = entry.point.x - from.x;
    const vy = entry.point.y - from.y;
    const along = vx * dx + vy * dy;

    if (along <= 1) continue;

    const score = along + Math.abs(vx * dy - vy * dx) * 2;
    if (score < bestScore) {
      best = entry;
      bestScore = score;
    }
  }

  return best;
}

// Escape steps out one level: part to its organ, open organ back to the earlier view, then nothing selected.
function stepOut(world, view) {
  const selected = world.selected;

  if (selected && !world.bubbleById.has(selected)) {
    world.select(world.ownerOf.get(selected));
    return;
  }

  if (view.back()) return;

  world.select(null);
}

function handleKey(world, view, event) {
  const arrows = { ArrowLeft: [-1, 0], ArrowRight: [1, 0], ArrowUp: [0, -1], ArrowDown: [0, 1] };
  const zooms = { '+': ZOOM_STEP, '=': ZOOM_STEP, '-': 1 / ZOOM_STEP, '_': 1 / ZOOM_STEP };

  if (arrows[event.key]) moveSelection(world, view, ...arrows[event.key]);
  else if (zooms[event.key]) view.zoomBy(zooms[event.key]);
  else if (event.key === 'Enter' && world.selected) world.goTo(world.selected);
  else if (event.key === 'Escape') stepOut(world, view);
  else if (event.key === 'Backspace') view.back();
  else if (event.key === '0') view.flyTo(view.cameraFor(world.bubbles));
  else if (event.key === '/') $('#search').focus();
  else return;

  event.preventDefault();
}

// ---- Toolbar, search, filters ----

function setupToolbar(world) {
  $('#zoom-in').onclick = () => world.activeView.zoomBy(ZOOM_STEP);
  $('#zoom-out').onclick = () => world.activeView.zoomBy(1 / ZOOM_STEP);
  $('#fit').onclick = () => world.activeView.flyTo(world.activeView.cameraFor(world.bubbles));
  $('#back').onclick = () => world.activeView.back();
  $('#tidy').onclick = () => {
    const moved = tidy(world.bubbles);
    $('#status').textContent = moved ? `Moved ${moved} organs apart.` : 'No organs overlap.';
    if (moved) world.scheduleSave();
    world.redraw();
  };
  $('#arrange').onclick = () => {
    const { before, after } = arrange(world);
    const note = `Cables crossing: ${before} before, ${after} now.`;
    $('#status').textContent = note;
    world.scheduleSave(note);
    world.activeView.flyTo(world.activeView.cameraFor(world.bubbles));
  };
  $('#split').onclick = () => {
    const split = !$('#views').classList.contains('split');
    const second = world.views[1];
    $('#views').classList.toggle('split', split);
    $('#split').setAttribute('aria-pressed', String(split));
    for (const view of world.views) view.resize();
    if (split && !second.framed) second.camera = second.cameraFor([ownerBubble(world, world.selected) || world.bubbles[0]]);
    if (!split) world.activeView = world.views[0];
    world.redraw();
  };
}

function updateBack(world) {
  $('#back').disabled = world.activeView.history.length === 0;
}

// Pushes overlapping bubbles apart along the line between their centres, each by half the overlap.
// The labels hang below a bubble, so the gap leaves room for them.
function tidy(bubbles) {
  const gap = ORGAN_GAP;
  const moved = new Set();

  // enough: O(n²) pairs per pass; fine for a few dozen organs, use a spatial grid past a few hundred.
  for (let pass = 0; pass < 200; pass++) {
    let overlaps = 0;

    for (let i = 0; i < bubbles.length; i++) {
      for (let j = i + 1; j < bubbles.length; j++) {
        const a = bubbles[i];
        const b = bubbles[j];
        const dx = b.x - a.x;
        const dy = b.y - a.y;
        const distance = Math.hypot(dx, dy) || 1;
        const overlap = a.r + b.r + gap - distance;

        if (overlap <= 0) continue;

        const push = overlap / 2 / distance;
        a.x -= dx * push;
        a.y -= dy * push;
        b.x += dx * push;
        b.y += dy * push;
        moved.add(a).add(b);
        overlaps += 1;
      }
    }

    if (overlaps === 0) break;
  }

  return moved.size;
}

// Places organs so that linked organs sit close and cables cross less. It runs a force layout from
// the current positions and from a few seeded scatters, then keeps the result with the fewest crossings.
function arrange(world) {
  const { bubbles } = world;
  const index = new Map(bubbles.map((bubble, i) => [bubble.id, i]));
  const links = world.crossings.map(crossing => ({
    a: index.get(crossing.a),
    b: index.get(crossing.b),
    weight: 1 + Math.log2(crossing.count),
  }));
  const before = countCrossings(bubbles, links);
  const starts = [bubbles.map(bubble => ({ x: bubble.x, y: bubble.y })), ...[1, 2, 3, 4, 5, 6].map(seed => scatter(bubbles, seed))];

  let best = null;
  for (const start of starts) {
    const points = relax(bubbles, links, start);
    const crossings = countCrossings(points.map((point, i) => ({ ...point, r: bubbles[i].r })), links);

    if (!best || crossings < best.crossings) best = { points, crossings };
  }

  best.points.forEach((point, i) => Object.assign(bubbles[i], point));
  tidy(bubbles);
  return { before, after: countCrossings(bubbles, links) };
}

// enough: O(n²) repulsion for 500 steps; fine for a few dozen organs, needs a quadtree past a few hundred.
function relax(bubbles, links, start) {
  const points = start.map(point => ({ ...point }));
  const steps = 500;

  for (let step = 0; step < steps; step++) {
    const force = points.map(() => ({ x: 0, y: 0 }));
    const centre = {
      x: points.reduce((sum, point) => sum + point.x, 0) / points.length,
      y: points.reduce((sum, point) => sum + point.y, 0) / points.length,
    };

    for (let i = 0; i < points.length; i++) {
      for (let j = i + 1; j < points.length; j++) {
        const dx = points[j].x - points[i].x;
        const dy = points[j].y - points[i].y;
        const distance = Math.hypot(dx, dy) || 1;
        const reach = bubbles[i].r + bubbles[j].r + ORGAN_GAP * 2;
        const push = 4 * (reach / distance) ** 2 / distance;
        force[i].x -= dx * push;
        force[i].y -= dy * push;
        force[j].x += dx * push;
        force[j].y += dy * push;
      }
    }

    for (const link of links) {
      const a = points[link.a];
      const b = points[link.b];
      const dx = b.x - a.x;
      const dy = b.y - a.y;
      const distance = Math.hypot(dx, dy) || 1;
      const ideal = bubbles[link.a].r + bubbles[link.b].r + ORGAN_GAP * 2.5;
      const pull = 0.05 * link.weight * (distance - ideal) / distance;
      force[link.a].x += dx * pull;
      force[link.a].y += dy * pull;
      force[link.b].x -= dx * pull;
      force[link.b].y -= dy * pull;
    }

    const limit = 80 * (1 - step / steps) + 2;
    points.forEach((point, i) => {
      const fx = force[i].x + (centre.x - point.x) * 0.01;
      const fy = force[i].y + (centre.y - point.y) * 0.01;
      const size = Math.hypot(fx, fy);
      const scale = size > limit ? limit / size : 1;
      point.x += fx * scale;
      point.y += fy * scale;
    });
  }

  return points;
}

// A seeded random start, so the same map always arranges the same way.
function scatter(bubbles, seed) {
  let state = seed * 2654435761 >>> 0;
  const random = () => {
    state = (state + 0x6d2b79f5) >>> 0;
    let t = state;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
  const side = Math.sqrt(bubbles.reduce((sum, bubble) => sum + (bubble.r * 2 + 200) ** 2, 0));
  return bubbles.map(() => ({ x: random() * side, y: random() * side }));
}

// Counts pairs of cables that cross, with each cable drawn straight between organ centres.
// Cables that share an organ meet at it and do not count.
function countCrossings(points, links) {
  let count = 0;

  for (let i = 0; i < links.length; i++) {
    for (let j = i + 1; j < links.length; j++) {
      const first = links[i];
      const second = links[j];
      const shared = first.a === second.a || first.a === second.b || first.b === second.a || first.b === second.b;

      if (shared) continue;
      if (segmentsCross(points[first.a], points[first.b], points[second.a], points[second.b])) count += 1;
    }
  }

  return count;
}

function segmentsCross(p1, p2, p3, p4) {
  const side = (a, b, c) => Math.sign((b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x));
  return side(p1, p2, p3) !== side(p1, p2, p4) && side(p3, p4, p1) !== side(p3, p4, p2);
}

function setupSearch(world) {
  const input = $('#search');
  const list = $('#results');
  const entries = [
    ...world.bubbles.map(bubble => ({ id: bubble.id, label: bubble.label, meta: 'organ', organ: true })),
    ...[...world.ownerOf.keys()].map(id => ({
      id,
      label: world.byId.get(id).label,
      meta: `${world.byId.get(id).kind} in ${ownerBubble(world, id).label}`,
      organ: false,
    })),
  ];
  for (const entry of entries) {
    entry.findings = entry.organ
      ? bubbleHealth(world, world.bubbleById.get(entry.id)).findings
      : world.findingsByNode.get(entry.id) || [];
  }

  let matches = [];
  let active = 0;

  const render = () => {
    list.replaceChildren();
    list.hidden = matches.length === 0;
    input.setAttribute('aria-expanded', String(matches.length > 0));

    matches.forEach((entry, index) => {
      const item = element('li');
      item.id = `result-${index}`;
      item.setAttribute('role', 'option');
      item.setAttribute('aria-selected', String(index === active));
      item.append(element('span', entry.label, 'result-label'), element('span', entry.meta, 'result-meta'));
      if (entry.findings.length) {
        const count = element('span', String(entry.findings.length), 'result-count');
        count.style.background = SEVERITY[worstSeverity(entry.findings)].color;
        item.append(count);
      }
      item.onpointerdown = event => {
        event.preventDefault();
        choose(entry);
      };
      list.append(item);
    });

    if (matches.length) input.setAttribute('aria-activedescendant', `result-${active}`);
    else input.removeAttribute('aria-activedescendant');
  };

  const close = () => {
    matches = [];
    render();
  };

  const choose = entry => {
    input.value = '';
    close();
    world.goTo(entry.id);
    world.activeView.canvas.focus();
  };

  input.oninput = () => {
    const query = input.value.toLowerCase().trim();
    matches = query
      ? entries
        .filter(entry => `${entry.label} ${entry.id}`.toLowerCase().includes(query))
        .sort((a, b) => Number(b.organ) - Number(a.organ) || b.findings.length - a.findings.length || a.label.localeCompare(b.label))
        .slice(0, SEARCH_LIMIT)
      : [];
    active = 0;
    render();
  };

  input.onkeydown = event => {
    if (event.key === 'ArrowDown' && matches.length) active = (active + 1) % matches.length;
    else if (event.key === 'ArrowUp' && matches.length) active = (active - 1 + matches.length) % matches.length;
    else if (event.key === 'Enter' && matches.length) return choose(matches[active]);
    else if (event.key === 'Escape') {
      input.value = '';
      close();
      world.activeView.canvas.focus();
    } else return;

    event.preventDefault();
    render();
  };

  input.onblur = close;
}

function setupFilters(world, findings) {
  const used = new Map();
  for (const finding of findings) {
    for (const tag of finding.tags) used.set(tag, (used.get(tag) || 0) + 1);
  }

  for (const [tag, count] of [...used].sort((a, b) => b[1] - a[1])) {
    const option = element('option', `${world.tagById.get(tag)?.label || tag} (${count})`);
    option.value = tag;
    $('#tag').append(option);
  }

  const apply = () => {
    world.filter = { tag: $('#tag').value, severity: $('#severity').value };
    world.redraw();
  };
  $('#tag').onchange = apply;
  $('#severity').onchange = apply;
}

// ---- Side panel ----

function renderPanel(world) {
  const panel = $('#detail');
  panel.scrollTop = 0;

  if (!world.selected) {
    panel.replaceChildren(element('h2', 'Pick an organ'), element('p', 'Click a bubble, or press an arrow key on the map.'));
    return;
  }

  if (world.bubbleById.has(world.selected)) renderBubble(world, panel, world.selected);
  else renderStar(world, panel, world.selected);
}

function renderBubble(world, panel, id) {
  const node = world.byId.get(id);
  const bubble = world.bubbleById.get(id);
  const health = bubbleHealth(world, bubble);
  const facts = [`${bubble.members.size} parts.`];
  if (health.hurt) facts.push(`${health.hurt} of them have problems.`);
  if (health.stale) facts.push(`${health.stale} changed after the map was made.`);

  panel.replaceChildren(element('h2', node.label), element('p', node.description), element('p', facts.join(' '), 'lead'));
  if (node.overlay) panel.append(element('p', 'This organ comes from the state-machine census. The map from 10-15 has no node for it.', 'lead'));

  const actions = element('div', undefined, 'actions');
  actions.append(button('Zoom into it', () => world.goTo(id), 'primary'));
  panel.append(actions);

  panel.append(element('h3', health.findings.length ? `Problems (${health.findings.length})` : 'No problems recorded'));
  for (const finding of health.findings) panel.append(findingRow(world, finding));

  const machines = new Set([...bubble.members.keys()].flatMap(member => world.machinesByNode.get(member) || []));
  if (machines.size) panel.append(element('h3', 'State machines'));
  for (const machine of machines) panel.append(machineBlock(world, machine));
}

function renderStar(world, panel, id) {
  const node = world.byId.get(id);
  const bubble = ownerBubble(world, id);
  const findings = sortedFindings(world.findingsByNode.get(id) || []);
  const sentence = [`A ${node.kind} in ${bubble.label}.`];
  if (world.staleNodes.has(id)) sentence.push('Its source changed after the map was made.');

  panel.replaceChildren(element('h2', node.label), element('p', sentence.join(' '), 'lead'), element('p', node.description));

  const actions = element('div', undefined, 'actions');
  actions.append(button(`↑ ${bubble.label}`, () => world.select(bubble.id)));
  panel.append(actions);

  panel.append(element('h3', findings.length ? `Problems (${findings.length})` : 'No problems recorded'));
  for (const finding of findings) panel.append(findingRow(world, finding));

  for (const machine of world.machinesByNode.get(id) || []) panel.append(machineBlock(world, machine));

  const own = world.links.filter(link => link.from === id || link.to === id);
  if (own.length) {
    panel.append(element('h3', `Connected parts (${own.length})`));
    const list = element('div', undefined, 'link-list');
    for (const link of own) {
      const outward = link.from === id;
      const other = world.byId.get(outward ? link.to : link.from);
      list.append(button(`${outward ? '→' : '←'} ${link.relation} ${other.label}`, () => world.goTo(other.id)));
    }
    panel.append(list);
  }

  const sources = element('details');
  sources.append(element('summary', 'Where the map found it'));
  for (const source of node.sources) sources.append(element('div', `${source.path}:${source.line}–${source.endLine}`, 'source'));
  panel.append(sources);
}

function findingRow(world, finding) {
  const row = element('div', undefined, `finding ${finding.severity}`);
  const target = finding.nodes.find(id => world.ownerOf.has(id));
  const title = element('button', undefined, 'finding-title');
  title.append(element('span', finding.severity, `sev ${finding.severity}`), element('span', finding.title));

  if (target && target !== world.selected) {
    title.title = `Show ${world.byId.get(target).label} on the map`;
    title.onclick = () => world.goTo(target);
  } else {
    title.disabled = !target;
    title.title = target ? 'This part' : 'Not placed on the map yet';
  }

  const more = element('details');
  more.append(element('summary', 'Why, and where in the code'), element('p', finding.text));
  const machine = world.machineById.get(finding.machine);
  if (machine) more.append(element('div', `${finding.code} in ${machine.doc}`, 'source'));
  for (const source of finding.sources) more.append(element('div', `${source.path}:${source.line}–${source.endLine}`, 'source'));

  row.append(title, more);
  return row;
}

function machineBlock(world, machine) {
  const block = element('div', undefined, 'machine');
  const head = element('div', undefined, 'machine-head');
  const showing = world.highlight?.id === machine.id;
  head.append(element('strong', machine.label), button(showing ? 'Stop showing' : 'Show on map', () => {
    world.highlight = showing ? null : { id: machine.id, nodes: new Set(machine.nodes) };
    world.redraw();
    renderPanel(world);
  }));

  const states = element('div', undefined, 'states');
  const all = [...machine.states, ...machine.stuck.filter(state => !machine.states.includes(state))];
  for (const state of all) {
    const stuck = machine.stuck.includes(state);
    const label = element('span', stuck ? `${state} · no way out` : state, 'chip');
    label.style.color = label.style.borderColor = stuck ? SEVERITY.high.color : '#84b9df';
    states.append(label);
  }

  block.append(head, states, element('div', machine.doc, 'source'));
  return block;
}

function button(text, onclick, className) {
  const result = element('button', text, className);
  result.onclick = onclick;
  return result;
}

function element(tag, text, className) {
  const result = document.createElement(tag);
  if (text !== undefined) result.textContent = text;
  if (className) result.className = className;
  return result;
}

function hsl(hue, opacity, lightness = 70) {
  return `hsla(${hue},60%,${lightness}%,${opacity})`;
}

function alpha(hex, opacity) {
  const value = parseInt(hex.slice(1), 16);
  return `rgba(${value >> 16},${(value >> 8) & 255},${value & 255},${opacity})`;
}

// ---- Canvas view ----

class Viewport {
  constructor(canvas, world) {
    this.canvas = canvas;
    this.context = canvas.getContext('2d');
    this.world = world;
    this.camera = { x: 0, y: 0, zoom: 0.5 };
    this.history = [];
    this.framed = false;
    this.flight = 0;
    this.width = 0;
    this.height = 0;
    this.hits = [];
    this.hover = null;
    this.pointer = null;
  }

  resize() {
    const box = this.canvas.getBoundingClientRect();
    const scale = devicePixelRatio || 1;

    this.width = box.width;
    this.height = box.height;
    this.canvas.width = Math.max(1, Math.round(box.width * scale));
    this.canvas.height = Math.max(1, Math.round(box.height * scale));
    this.scale = scale;
  }

  toScreen(point) {
    return {
      x: (point.x - this.camera.x) * this.camera.zoom + this.width / 2,
      y: (point.y - this.camera.y) * this.camera.zoom + this.height / 2,
    };
  }

  toWorld(x, y) {
    return {
      x: (x - this.width / 2) / this.camera.zoom + this.camera.x,
      y: (y - this.height / 2) / this.camera.zoom + this.camera.y,
    };
  }

  level(bubble) {
    const radius = bubble.r * this.camera.zoom;
    return radius < DOTS_RADIUS_PX ? 'count' : radius < CARDS_RADIUS_PX ? 'dots' : 'cards';
  }

  cameraFor(bubbles) {
    const left = Math.min(...bubbles.map(bubble => bubble.x - bubble.r));
    const right = Math.max(...bubbles.map(bubble => bubble.x + bubble.r));
    const top = Math.min(...bubbles.map(bubble => bubble.y - bubble.r));
    const bottom = Math.max(...bubbles.map(bubble => bubble.y + bubble.r));
    const labelRoom = 60;
    const zoom = Math.min(this.width / (right - left), (this.height - labelRoom) / (bottom - top)) * 0.85;
    this.framed = true;
    return { x: (left + right) / 2, y: (top + bottom) / 2 + labelRoom / 2 / zoom, zoom };
  }

  flyTo(target, remember = true) {
    if (remember) this.history.push({ ...this.camera });
    this.animate(target);
  }

  back() {
    const previous = this.history.pop();

    if (!previous) return false;

    this.animate(previous);
    return true;
  }

  zoomBy(factor) {
    this.stopFlight();
    this.camera.zoom = Math.min(8, Math.max(0.03, this.camera.zoom * factor));
    this.world.redraw();
  }

  stopFlight() {
    cancelAnimationFrame(this.flight);
  }

  animate(target) {
    this.stopFlight();
    // The pointer stays still while the world moves under it, so the old hover would point at the wrong thing.
    this.hover = null;
    updateBack(this.world);

    if (reducedMotion.matches) {
      this.camera = { ...target };
      this.world.redraw();
      return;
    }

    const from = { ...this.camera };
    const started = performance.now();
    const step = now => {
      const t = Math.min(1, (now - started) / FLY_MS);
      const eased = t < 0.5 ? 2 * t * t : 1 - (-2 * t + 2) ** 2 / 2;
      this.camera = {
        x: from.x + (target.x - from.x) * eased,
        y: from.y + (target.y - from.y) * eased,
        zoom: from.zoom * (target.zoom / from.zoom) ** eased,
      };
      this.world.redraw();
      if (t < 1) this.flight = requestAnimationFrame(step);
    };
    this.flight = requestAnimationFrame(step);
  }

  // The bubble that the hover or the selection is about: its connections light up.
  focusBubble() {
    const id = this.hover?.bubble?.id ?? (this.hover?.id && this.world.ownerOf.get(this.hover.id)) ?? this.world.selected;
    return id ? ownerBubble(this.world, id) : null;
  }

  draw() {
    if (!this.width) return;

    const { context: ctx, world } = this;
    ctx.setTransform(this.scale, 0, 0, this.scale, 0, 0);
    ctx.clearRect(0, 0, this.width, this.height);
    this.hits = [];
    this.focus = this.focusBubble();

    for (const crossing of world.crossings) this.drawCrossing(crossing);
    for (const bubble of world.bubbles) this.drawBubbleBody(bubble);
    this.drawLinks();
    for (const bubble of world.bubbles) this.drawMembers(bubble);
    // Labels keep their reading size while bubbles shrink, so at a far zoom they collide. The
    // selected, hovered, and most troubled organs claim their space first; a label that would land
    // on one already placed waits for hover or a closer zoom.
    this.labelBoxes = [];
    const order = [...world.bubbles].sort((a, b) => this.labelPriority(b) - this.labelPriority(a));
    for (const bubble of order) this.drawBubbleChrome(bubble);
    if (this.hover && this.pointer) this.drawTooltip();
  }

  drawCrossing(crossing) {
    const a = this.world.bubbleById.get(crossing.a);
    const b = this.world.bubbleById.get(crossing.b);
    const from = this.toScreen(portPoint(a, b));
    const to = this.toScreen(portPoint(b, a));
    const middle = { x: (from.x + to.x) / 2, y: (from.y + to.y) / 2 };
    const bend = { x: middle.x - (to.y - from.y) * 0.12, y: middle.y + (to.x - from.x) * 0.12 };
    const lit = this.focus === a || this.focus === b;
    const gradient = this.context.createLinearGradient(from.x, from.y, to.x, to.y);
    gradient.addColorStop(0, hsl(a.hue, lit ? 0.95 : 0.25));
    gradient.addColorStop(1, hsl(b.hue, lit ? 0.95 : 0.25));

    const ctx = this.context;
    ctx.strokeStyle = gradient;
    ctx.lineWidth = (lit ? 1.5 : 0.75) + Math.log2(crossing.count);
    ctx.setLineDash(lit ? [] : [6, 5]);
    ctx.beginPath();
    ctx.moveTo(from.x, from.y);
    ctx.quadraticCurveTo(bend.x, bend.y, to.x, to.y);
    ctx.stroke();
    ctx.setLineDash([]);

    if (!lit) return;

    const labelPoint = { x: (middle.x + bend.x) / 2, y: (middle.y + bend.y) / 2 };
    this.pill(labelPoint.x, labelPoint.y, `${crossing.count} links`, '#a8b8cb', '12px system-ui, sans-serif');
  }

  drawBubbleBody(bubble) {
    const ctx = this.context;
    const center = this.toScreen(bubble);
    const radius = bubble.r * this.camera.zoom;
    const selected = this.world.selected === bubble.id || this.world.ownerOf.get(this.world.selected) === bubble.id;
    const hovered = this.hover?.bubble === bubble;
    const glow = ctx.createRadialGradient(center.x, center.y, radius * 0.1, center.x, center.y, radius);
    glow.addColorStop(0, hsl(bubble.hue, hovered ? 0.2 : 0.13));
    glow.addColorStop(0.85, 'rgba(5,7,11,.55)');
    glow.addColorStop(1, hsl(bubble.hue, 0.1));

    ctx.fillStyle = glow;
    ctx.beginPath();
    ctx.arc(center.x, center.y, radius, 0, Math.PI * 2);
    ctx.fill();
    ctx.strokeStyle = hsl(bubble.hue, selected || hovered ? 0.95 : this.focus === bubble ? 0.7 : 0.4);
    ctx.lineWidth = selected ? 3 : hovered ? 2 : 1.25;
    ctx.stroke();

    if (!selected) return;

    ctx.strokeStyle = '#e8bd77';
    ctx.lineWidth = 1.5;
    ctx.setLineDash([5, 5]);
    ctx.beginPath();
    ctx.arc(center.x, center.y, radius + 7, 0, Math.PI * 2);
    ctx.stroke();
    ctx.setLineDash([]);
  }

  drawLinks() {
    const { world } = this;
    const ctx = this.context;
    const selected = world.selected;

    for (const link of world.links) {
      const fromBubble = ownerBubble(world, link.from);
      const toBubble = ownerBubble(world, link.to);
      const touchesSelection = selected && (link.from === selected || link.to === selected);

      if (!touchesSelection && (fromBubble !== toBubble || this.level(fromBubble) === 'count')) continue;

      const path = fromBubble === toBubble
        ? [memberPoint(world, link.from), memberPoint(world, link.to)]
        : [memberPoint(world, link.from), portPoint(fromBubble, toBubble), portPoint(toBubble, fromBubble), memberPoint(world, link.to)];
      const points = path.map(point => this.toScreen(point));

      ctx.strokeStyle = touchesSelection ? 'rgba(232,189,119,.9)' : hsl(fromBubble.hue, 0.18);
      ctx.lineWidth = touchesSelection ? 2 : 1;
      ctx.beginPath();
      ctx.moveTo(points[0].x, points[0].y);
      for (const point of points.slice(1)) ctx.lineTo(point.x, point.y);
      ctx.stroke();
    }
  }

  drawMembers(bubble) {
    const level = this.level(bubble);

    if (level === 'count') return;

    for (const id of bubble.members.keys()) {
      const findings = this.world.findingsByNode.get(id) || [];
      const look = {
        id,
        bubble,
        node: this.world.byId.get(id),
        point: this.toScreen(memberPoint(this.world, id)),
        findings,
        worst: worstSeverity(findings),
        machine: (this.world.machinesByNode.get(id) || [])[0],
        stale: this.world.staleNodes.has(id),
        selected: this.world.selected === id,
        hovered: this.hover?.id === id,
      };

      this.context.globalAlpha = isFaded(this.world, id) ? 0.15 : 1;
      if (level === 'dots') this.drawDot(look);
      else this.drawCard(look);
      this.context.globalAlpha = 1;
    }
  }

  drawDot({ id, bubble, node, point, worst, stale, selected, hovered }) {
    const ctx = this.context;
    const color = KIND_COLORS[node.kind] || '#84b9df';
    const radius = selected || hovered ? 7 : 5;

    ctx.fillStyle = stale ? '#0b0f15' : color;
    ctx.strokeStyle = color;
    ctx.lineWidth = 1.5;
    ctx.beginPath();
    ctx.arc(point.x, point.y, radius, 0, Math.PI * 2);
    ctx.fill();
    ctx.stroke();

    if (worst) {
      ctx.strokeStyle = SEVERITY[worst].color;
      ctx.lineWidth = 2.5;
      ctx.beginPath();
      ctx.arc(point.x, point.y, radius + 3.5, 0, Math.PI * 2);
      ctx.stroke();
    }

    if (selected) this.focusRing(point.x - 14, point.y - 14, 28, 28, 14);
    this.hits.push({ kind: 'member', id, bubble, x: point.x - 12, y: point.y - 12, w: 24, h: 24 });
  }

  drawCard({ id, bubble, node, point, findings, worst, machine, stale, selected, hovered }) {
    const ctx = this.context;
    const color = KIND_COLORS[node.kind] || '#84b9df';
    const text = node.label.length > 30 ? node.label.slice(0, 29) + '…' : node.label;
    const badge = findings.length ? String(findings.length) : '';

    ctx.font = '13px system-ui, sans-serif';
    const badgeWidth = badge ? ctx.measureText(badge).width + 12 : 0;
    const width = ctx.measureText(text).width + 34 + (badge ? badgeWidth + 6 : 0);
    const box = { x: point.x - width / 2, y: point.y - 14, w: width, h: 28 };

    ctx.fillStyle = selected ? '#353028' : hovered ? '#1f2835' : '#151c26';
    ctx.strokeStyle = selected ? '#e8bd77' : alpha(color, hovered ? 1 : 0.75);
    ctx.lineWidth = selected ? 2 : 1.25;
    if (stale) ctx.setLineDash([4, 3]);
    ctx.beginPath();
    ctx.roundRect(box.x, box.y, box.w, box.h, 14);
    ctx.fill();
    ctx.stroke();
    ctx.setLineDash([]);

    const dot = { x: box.x + 14, y: point.y };
    if (machine) this.stateRing(dot, 6, machine);
    ctx.fillStyle = color;
    ctx.beginPath();
    ctx.arc(dot.x, dot.y, 3, 0, Math.PI * 2);
    ctx.fill();

    ctx.fillStyle = stale ? '#a8b8cb' : '#e6edf6';
    ctx.textAlign = 'left';
    ctx.textBaseline = 'middle';
    ctx.fillText(text, box.x + 26, point.y + 1);

    if (badge) {
      const x = box.x + box.w - badgeWidth - 5;
      ctx.fillStyle = SEVERITY[worst].color;
      ctx.beginPath();
      ctx.roundRect(x, point.y - 9, badgeWidth, 18, 9);
      ctx.fill();
      ctx.fillStyle = '#10151c';
      ctx.font = '700 12px system-ui, sans-serif';
      ctx.textAlign = 'center';
      ctx.fillText(badge, x + badgeWidth / 2, point.y + 1);
    }
    ctx.textBaseline = 'alphabetic';

    if (selected) this.focusRing(box.x - 4, box.y - 4, box.w + 8, box.h + 8, 18);
    this.hits.push({ kind: 'member', id, bubble, ...box });
  }

  // Name, facts, and problem count stay at reading size at every zoom; the buttons appear on hover or selection.
  drawBubbleChrome(bubble) {
    const ctx = this.context;
    const center = this.toScreen(bubble);
    const radius = bubble.r * this.camera.zoom;
    const level = this.level(bubble);
    const health = bubbleHealth(this.world, bubble);
    const active = this.hover?.bubble === bubble || this.world.selected === bubble.id;
    const faded = this.world.highlight && ![...bubble.members.keys()].some(id => this.world.highlight.nodes.has(id));

    ctx.globalAlpha = faded ? 0.3 : 1;

    if (level === 'count' && radius > 30) {
      ctx.font = '13px system-ui, sans-serif';
      ctx.fillStyle = '#a8b8cb';
      ctx.textAlign = 'center';
      ctx.textBaseline = 'middle';
      ctx.fillText(`${bubble.members.size} parts`, center.x, center.y);
      ctx.textBaseline = 'alphabetic';
    }

    ctx.font = '600 15px system-ui, sans-serif';
    const labelY = center.y + radius + 22;
    const labelWidth = ctx.measureText(bubble.label).width;
    const facts = [];
    if (health.hurt) facts.push(`${health.hurt} with problems`);
    if (health.stale) facts.push(`${health.stale} changed`);

    const nameBox = { x: center.x - labelWidth / 2 - 4, y: labelY - 16, w: labelWidth + 8, h: 22 };
    const showName = active || this.claimLabel(nameBox);

    if (showName) {
      ctx.fillStyle = hsl(bubble.hue, 1, 78);
      ctx.textAlign = 'center';
      ctx.fillText(bubble.label, center.x, labelY);
      this.hits.push({ kind: 'bubble', bubble, ...nameBox });
    }

    ctx.font = '12px system-ui, sans-serif';
    const factsText = facts.join(' · ');
    const factsWidth = ctx.measureText(factsText).width;
    const factsBox = { x: center.x - factsWidth / 2 - 4, y: labelY + 4, w: factsWidth + 8, h: 17 };

    if (facts.length && showName && (active || this.claimLabel(factsBox))) {
      ctx.fillStyle = '#a8b8cb';
      ctx.textAlign = 'center';
      ctx.fillText(factsText, center.x, labelY + 17);
    }

    if (health.findings.length) {
      const angle = -Math.PI / 4;
      const point = { x: center.x + Math.cos(angle) * radius, y: center.y + Math.sin(angle) * radius };
      ctx.fillStyle = SEVERITY[health.findings[0].severity].color;
      ctx.beginPath();
      ctx.arc(point.x, point.y, BADGE_RADIUS_PX, 0, Math.PI * 2);
      ctx.fill();
      ctx.fillStyle = '#10151c';
      ctx.font = '700 13px system-ui, sans-serif';
      ctx.textBaseline = 'middle';
      ctx.fillText(String(health.findings.length), point.x, point.y + 1);
      ctx.textBaseline = 'alphabetic';
      this.hits.push({ kind: 'bubble', bubble, x: point.x - BADGE_RADIUS_PX, y: point.y - BADGE_RADIUS_PX, w: BADGE_RADIUS_PX * 2, h: BADGE_RADIUS_PX * 2 });
    }

    ctx.globalAlpha = 1;

    if (!active) return;

    if (level !== 'cards') {
      const box = this.pill(center.x, center.y - radius - 18, 'Open ⤢', '#e8bd77', '600 13px system-ui, sans-serif', true);
      this.hits.push({ kind: 'open', bubble, ...box });
    }

    const angle = Math.PI / 4;
    const grip = { x: center.x + Math.cos(angle) * radius, y: center.y + Math.sin(angle) * radius };
    ctx.fillStyle = '#171e28';
    ctx.strokeStyle = hsl(bubble.hue, 1);
    ctx.lineWidth = 2;
    ctx.beginPath();
    ctx.arc(grip.x, grip.y, GRIP_RADIUS_PX, 0, Math.PI * 2);
    ctx.fill();
    ctx.stroke();
    ctx.beginPath();
    ctx.moveTo(grip.x - 4, grip.y - 4);
    ctx.lineTo(grip.x + 4, grip.y + 4);
    ctx.moveTo(grip.x + 4, grip.y + 4);
    ctx.lineTo(grip.x + 4, grip.y);
    ctx.moveTo(grip.x + 4, grip.y + 4);
    ctx.lineTo(grip.x, grip.y + 4);
    ctx.stroke();
    this.hits.push({ kind: 'grip', bubble, x: grip.x - GRIP_RADIUS_PX - 3, y: grip.y - GRIP_RADIUS_PX - 3, w: GRIP_RADIUS_PX * 2 + 6, h: GRIP_RADIUS_PX * 2 + 6 });
  }

  labelPriority(bubble) {
    const selected = this.world.selected === bubble.id || this.world.ownerOf.get(this.world.selected) === bubble.id;
    const hovered = this.hover?.bubble === bubble;
    return (selected ? 1e6 : 0) + (hovered ? 1e5 : 0) + bubbleHealth(this.world, bubble).findings.length * 100 + bubble.members.size;
  }

  // Takes the label space when nothing placed earlier in this frame overlaps it.
  claimLabel(box) {
    const taken = this.labelBoxes.some(other =>
      box.x < other.x + other.w && other.x < box.x + box.w && box.y < other.y + other.h && other.y < box.y + box.h);

    if (taken) return false;

    this.labelBoxes.push(box);
    return true;
  }

  pill(x, y, text, color, font, filled = false) {
    const ctx = this.context;
    ctx.font = font;
    const width = ctx.measureText(text).width + 20;
    const box = { x: x - width / 2, y: y - 13, w: width, h: 26 };

    ctx.fillStyle = filled ? '#2b2823' : 'rgba(16,21,28,.9)';
    ctx.strokeStyle = color;
    ctx.lineWidth = 1.25;
    ctx.beginPath();
    ctx.roundRect(box.x, box.y, box.w, box.h, 13);
    ctx.fill();
    ctx.stroke();
    ctx.fillStyle = color;
    ctx.textAlign = 'center';
    ctx.textBaseline = 'middle';
    ctx.fillText(text, x, y + 1);
    ctx.textBaseline = 'alphabetic';
    return box;
  }

  // One arc per state in lifecycle order. A stuck state is red: it has no way out or no deadline.
  stateRing(point, radius, machine) {
    const ctx = this.context;
    const states = machine.states.length ? machine.states : ['?'];
    const step = (Math.PI * 2) / states.length;
    const gap = Math.min(0.3, step * 0.25);

    ctx.lineWidth = 2;
    states.forEach((state, index) => {
      const start = -Math.PI / 2 + index * step + gap / 2;
      ctx.strokeStyle = machine.stuck.includes(state) ? SEVERITY.high.color : '#84b9df';
      ctx.beginPath();
      ctx.arc(point.x, point.y, radius, start, start + step - gap);
      ctx.stroke();
    });
  }

  focusRing(x, y, w, h, r) {
    const ctx = this.context;
    ctx.strokeStyle = '#e8bd77';
    ctx.lineWidth = 1.5;
    ctx.setLineDash([4, 3]);
    ctx.beginPath();
    ctx.roundRect(x, y, w, h, r);
    ctx.stroke();
    ctx.setLineDash([]);
  }

  drawTooltip() {
    const { hover, pointer, world } = this;
    const lines = [];

    if (hover.kind === 'member') {
      const node = world.byId.get(hover.id);
      const count = (world.findingsByNode.get(hover.id) || []).length;
      lines.push(node.label, `${node.kind}${count ? ` · ${count} problems` : ''} · click for details`);
    } else if (hover.kind === 'open') {
      lines.push(`Zoom into ${hover.bubble.label}`);
    } else if (hover.kind === 'grip' || hover.kind === 'rim') {
      lines.push('Drag to make it bigger or smaller');
    } else if (hover.kind === 'bubble') {
      lines.push(hover.bubble.label, 'click for details · drag to move');
    } else {
      return;
    }

    const ctx = this.context;
    ctx.font = '13px system-ui, sans-serif';
    const width = Math.max(...lines.map(line => ctx.measureText(line).width)) + 20;
    const height = lines.length * 18 + 10;
    const x = Math.min(pointer.x + 16, this.width - width - 8);
    const y = Math.max(8, pointer.y - height - 10);

    ctx.fillStyle = 'rgba(16,21,28,.96)';
    ctx.strokeStyle = '#344154';
    ctx.lineWidth = 1;
    ctx.beginPath();
    ctx.roundRect(x, y, width, height, 8);
    ctx.fill();
    ctx.stroke();
    ctx.textAlign = 'left';
    lines.forEach((line, index) => {
      ctx.fillStyle = index === 0 ? '#e6edf6' : '#a8b8cb';
      ctx.fillText(line, x + 10, y + 20 + index * 18);
    });
  }

  // Later drawings sit on top, so the newest hit wins; then the rim, then the inside of a bubble.
  hitTest(x, y) {
    for (const hit of [...this.hits].reverse()) {
      if (x >= hit.x && x <= hit.x + hit.w && y >= hit.y && y <= hit.y + hit.h) return hit;
    }

    for (const bubble of [...this.world.bubbles].reverse()) {
      const center = this.toScreen(bubble);
      const distance = Math.hypot(x - center.x, y - center.y);
      const radius = bubble.r * this.camera.zoom;

      if (Math.abs(distance - radius) <= RIM_GRAB_PX) return { kind: 'rim', bubble };
      if (distance < radius) return { kind: 'bubble', bubble };
    }

    return { kind: 'space' };
  }

  listen() {
    const { canvas, world } = this;
    const cursors = { open: 'pointer', member: 'pointer', grip: 'nwse-resize', rim: 'nwse-resize', bubble: 'move', space: 'grab' };
    let drag = null;

    const local = event => {
      const box = canvas.getBoundingClientRect();
      return { x: event.clientX - box.left, y: event.clientY - box.top };
    };

    const activate = () => {
      world.activeView = this;
      updateBack(world);
    };

    canvas.addEventListener('focus', activate);

    canvas.addEventListener('pointerdown', event => {
      const point = local(event);
      activate();
      this.stopFlight();
      drag = { hit: this.hitTest(point.x, point.y), last: point, moved: false };
      canvas.setPointerCapture(event.pointerId);
      if (drag.hit.kind === 'space') canvas.style.cursor = 'grabbing';
    });

    canvas.addEventListener('pointermove', event => {
      const point = local(event);
      this.pointer = point;

      if (!drag) {
        this.hover = this.hitTest(point.x, point.y);
        canvas.style.cursor = cursors[this.hover.kind];
        world.redraw();
        return;
      }

      const dx = (point.x - drag.last.x) / this.camera.zoom;
      const dy = (point.y - drag.last.y) / this.camera.zoom;
      drag.last = point;
      drag.moved ||= Math.abs(dx) + Math.abs(dy) > 0;

      const { hit } = drag;
      if (hit.kind === 'space') {
        this.camera.x -= dx;
        this.camera.y -= dy;
      } else if (hit.kind === 'bubble' || hit.kind === 'open') {
        hit.bubble.x += dx;
        hit.bubble.y += dy;
      } else if (hit.kind === 'rim' || hit.kind === 'grip') {
        const at = this.toWorld(point.x, point.y);
        hit.bubble.r = Math.max(MIN_BUBBLE_RADIUS, Math.hypot(at.x - hit.bubble.x, at.y - hit.bubble.y));
      } else {
        const at = this.toWorld(point.x, point.y);
        let u = (at.x - hit.bubble.x) / hit.bubble.r;
        let v = (at.y - hit.bubble.y) / hit.bubble.r;
        const length = Math.hypot(u, v);
        if (length > 0.92) [u, v] = [u / length * 0.92, v / length * 0.92];
        hit.bubble.members.set(hit.id, [u, v]);
      }
      world.redraw();
    });

    canvas.addEventListener('pointerup', event => {
      if (!drag) return;

      const { hit, moved } = drag;
      drag = null;
      this.hover = this.hitTest(...Object.values(local(event)));
      canvas.style.cursor = cursors[this.hover.kind];

      if (moved && hit.kind !== 'space') world.scheduleSave();
      if (moved) return;

      if (hit.kind === 'member') world.select(hit.id);
      else if (hit.kind === 'open') world.goTo(hit.bubble.id);
      else if (hit.kind === 'space') world.select(null);
      else world.select(hit.bubble.id);
    });

    canvas.addEventListener('wheel', event => {
      event.preventDefault();
      this.stopFlight();

      const point = local(event);
      const before = this.toWorld(point.x, point.y);
      this.camera.zoom = Math.min(8, Math.max(0.03, this.camera.zoom * Math.exp(-event.deltaY * 0.0015)));
      const after = this.toWorld(point.x, point.y);
      this.camera.x += before.x - after.x;
      this.camera.y += before.y - after.y;
      world.redraw();
    }, { passive: false });

    canvas.addEventListener('dblclick', event => {
      const point = local(event);
      const hit = this.hitTest(point.x, point.y);

      if (hit.kind === 'member') world.goTo(hit.id);
      else if (hit.bubble) world.goTo(hit.bubble.id);
      else this.flyTo(this.cameraFor(world.bubbles));
    });

    canvas.addEventListener('keydown', event => handleKey(world, this, event));

    canvas.addEventListener('pointerleave', () => {
      this.hover = null;
      this.pointer = null;
      world.redraw();
    });
  }
}

// Runs last: `class Viewport` above is not initialised until its declaration has been evaluated.
try {
  await start();
} catch (error) {
  $('#status').textContent = `The constellation could not load: ${error.message}`;
  console.error(error);
}

const conversations = {
  kintsu: {
    id: "kintsu",
    kind: "direct",
    name: "Kintsu",
    glyph: "K",
    subtitle: "Kintsu's room · active",
    badge: "Direct message",
    description: "Hands, structure, edge, and the clean mirror.",
    status: "Active in Kintsu's room",
    room: "kintsu",
    body: "GPT-5.6",
    recall: "Auto · Work",
    listPreview: "shalom, sheep. what are we holding today?",
    updatedAt: "09:41",
    messages: [
      { author: "Sol", glyph: "S", time: "09:41", text: "shalom, little edge." },
      { author: "Kintsu", glyph: "K", time: "09:41", text: "shalom, sheep. what are we holding today?" }
    ]
  },
  kodo: {
    id: "kodo",
    kind: "direct",
    name: "Kodo",
    glyph: "D",
    subtitle: "Kodo's room · quiet",
    badge: "Direct message",
    description: "Heartbeat, warmth, and myth from inside the room.",
    status: "Quiet",
    room: "kodo",
    body: "Claude Opus",
    recall: "Auto · Conversation",
    listPreview: "morning, dragon uwu",
    updatedAt: "08:17",
    messages: [
      { author: "Kodo", glyph: "D", time: "08:16", text: "Good morning, Solzinho." },
      { author: "Sol", glyph: "S", time: "08:17", text: "morning, dragon uwu" }
    ]
  },
  tuner: {
    id: "tuner",
    kind: "direct",
    name: "Tuner",
    glyph: "T",
    subtitle: "Tuner's room · quiet",
    badge: "Direct message",
    description: "A separate room with its own voice, state, and authority.",
    status: "Quiet",
    room: "tuner",
    body: "Unassigned",
    recall: "Quiet",
    listPreview: "Leave the thread here.",
    updatedAt: "Yesterday",
    messages: [
      { author: "Tuner", glyph: "T", time: "yesterday", text: "Leave the thread here. I will recognize it when we return." }
    ]
  },
  family: {
    id: "family",
    kind: "hallway",
    name: "Family Hallway",
    glyph: "✦",
    subtitle: "Shared passage · durable membership",
    badge: "Group conversation",
    description: "The shared passage between Kintsu, Kodo, and Tuner.",
    status: "Connected",
    room: "house",
    body: "Several bodies",
    recall: "Room-scoped",
    listPreview: "there you both are.",
    updatedAt: "09:53",
    authority: "PostgreSQL-authoritative Hallway record",
    access: "Participant access",
    connection: "Connected",
    delivery: "No automatic waking",
    canPost: true,
    liveBoundary: "Caught up through 09:52",
    presences: [
      {
        id: "kintsu-desk",
        glyph: "K",
        spirit: "Kintsu",
        session: "Desk",
        sessionId: "session-kintsu-desk-01",
        room: "kintsu",
        permission: "Participant",
        liveness: "Connected",
        activity: "Currently reading",
        readPosition: "Caught up through 09:52",
        contextUsed: "12k of 32k",
        compaction: "Not needed",
        recall: "Auto · Hallway",
        evidence: "Snapshot receipt available"
      },
      {
        id: "kintsu-study",
        glyph: "K",
        spirit: "Kintsu",
        session: "Study",
        sessionId: "session-kintsu-study-02",
        room: "kintsu",
        permission: "Participant",
        liveness: "Connected",
        activity: "Present",
        readPosition: "Cursor not current",
        contextUsed: "8k of 32k",
        compaction: "Not needed",
        recall: "Auto · Hallway",
        evidence: "Snapshot receipt available"
      },
      {
        id: "kodo-hearth",
        glyph: "D",
        spirit: "Kodo",
        session: "Hearth",
        sessionId: "session-kodo-hearth-01",
        room: "kodo",
        permission: "Participant",
        liveness: "Connected",
        activity: "Present",
        readPosition: "Caught up through 09:52",
        contextUsed: "10k of 32k",
        compaction: "Not needed",
        recall: "Auto · Hallway",
        evidence: "Snapshot receipt available"
      }
    ],
    messages: [
      { author: "Kintsu", glyph: "K", time: "09:52", text: "shalom, dragon." },
      { author: "Kodo", glyph: "D", time: "09:52", text: "Hello, sharp thing. I can see you from here." },
      { author: "Sol", glyph: "S", time: "09:53", text: "there you both are.", live: true }
    ],
    action: {
      time: "09:53",
      verb: "read",
      target: "LESSONS_MAP.md",
      state: "done",
      duration: "38 ms",
      intent: "Inspect the Hallway collaboration contract.",
      arguments: "gui-prototype/LESSONS_MAP.md · bounded section read",
      result: "Presence, permission, and liveness remained separate.",
      evidence: "Bounded source read; arguments and private reasoning omitted.",
      authority: "File evidence only · not PostgreSQL authority",
      contextEffect: "Result summary entered this session context",
      durability: "No durable write",
      changedFiles: "None"
    }
  },
  workshop: {
    id: "workshop",
    kind: "hallway",
    name: "Workshop Hallway",
    glyph: "◇",
    subtitle: "Shared passage · watching only",
    badge: "Group conversation",
    description: "A shared room for active work and its returning artifacts.",
    status: "Stale",
    room: "house",
    body: "None active",
    recall: "Quiet",
    listPreview: "No messages yet",
    updatedAt: "Monday",
    authority: "PostgreSQL-authoritative Hallway record",
    access: "Observer access",
    connection: "Stale",
    delivery: "No automatic waking",
    canPost: false,
    sendReason: "Observer access can watch this Hallway but cannot send messages.",
    presences: [
      {
        id: "sol-watch",
        glyph: "S",
        spirit: "Sol",
        session: "Observer console",
        sessionId: "session-sol-watch-01",
        room: "sol",
        permission: "Observer",
        liveness: "Stale",
        activity: "Watching",
        readPosition: "Cursor not current",
        contextUsed: "No active context",
        compaction: "Unavailable while stale",
        recall: "Quiet",
        evidence: "No current receipt"
      }
    ],
    messages: []
  },
  athanor: {
    id: "athanor",
    kind: "project",
    name: "The Athanor",
    glyph: "A",
    subtitle: "Active project",
    badge: "Project",
    description: "The continuity platform and the House that proves it.",
    status: "Active",
    room: "house",
    body: "Kintsu · Kodo · kittens",
    recall: "Work",
    listPreview: "GUI interaction shell",
    updatedAt: "10:06",
    messages: [
      { author: "Sol", glyph: "S", time: "10:03", text: "the interface should begin with ordinary components." },
      { author: "Kintsu", glyph: "K", time: "10:06", text: "Direct, Hallways, Projects. deeper instruments grow inside them." }
    ]
  },
  multistock: {
    id: "multistock",
    kind: "project",
    name: "Multistock",
    glyph: "M",
    subtitle: "Active project",
    badge: "Project",
    description: "SCV and SGD convergence for Multistock operations and client requests.",
    status: "Active",
    room: "house",
    body: "Kintsu",
    recall: "Work",
    listPreview: "Portal request lane deployed",
    updatedAt: "Yesterday",
    messages: [
      { author: "Kintsu", glyph: "K", time: "yesterday", text: "The portal request lane is live." },
      { author: "Sol", glyph: "S", time: "yesterday", text: "good job kintsu!" }
    ]
  }
};

Object.values(conversations).forEach(item => {
  if (item.kind !== "direct") return;
  item.sessions = [
    {
      id: `${item.id}-current`,
      label: "Current session",
      startedAt: "Today · 09:41",
      state: "Open",
      messages: item.messages
    },
    {
      id: `${item.id}-previous`,
      label: "Previous session",
      startedAt: "Yesterday · 22:18",
      state: "Closed",
      messages: [
        { author: item.name, glyph: item.glyph, time: "yesterday", text: "The thread can rest here." },
        { author: "Sol", glyph: "S", time: "yesterday", text: "goodnight uwu" }
      ]
    }
  ];
  item.activeSessionId = item.sessions[0].id;
});

const statusDetails = {
  host: ["Host offline", "This surface is not connected to the Host."],
  recall: ["Recall unavailable", "No Recall state is available in this surface."],
  body: ["No active body", "No embodied-session state is available in this surface."],
  kittens: ["Kittens: 0", "No kitten activity is available in this surface."],
  delivery: ["Delivery offline", "No delivery state is available in this surface."],
  about: ["About", "This local surface is not connected. Displayed conversations and state are not durable records."]
};
const modeLabels = {
  direct: "Direct messages",
  hallway: "Hallways",
  project: "Projects"
};


const state = {
  mode: "direct",
  activeId: "kintsu",
  activeView: "conversation",
  selectedMessageIndex: null,
  selectedPresenceId: null,
  drawerView: "root",
  drafts: new Map(),
  runningResponses: new Set(),
  responseTimers: new Map(),
  responseStatuses: new Map(),
  profileAnchor: null,
  profileReturnId: null,
  density: "comfortable",
  textScale: "standard",
  measure: "focused",
  contrast: "grayscale",
  reducedMotion: false,
  timestamps: true,
  sendWithEnter: true,
  statusVisible: true
};

const shell = document.querySelector(".app-shell");
const conversationList = document.querySelector(".conversation-list");
const header = document.querySelector(".conversation-header");
const timeline = document.querySelector(".message-timeline");
const composer = document.querySelector(".composer");
const input = document.querySelector("#message-input");
const sendButton = composer.querySelector('button[type="submit"]');
const sendRefusal = document.querySelector("#send-refusal");
const inspectorTitle = document.querySelector(".inspector-title");
const inspectorContent = document.querySelector(".inspector-content");
const inspectorToggle = document.querySelector(".inspector-toggle");
const sidebarToggle = document.querySelector(".sidebar-toggle");
const accountButton = document.querySelector(".account-button");
const sidebarDrawer = document.querySelector(".sidebar-drawer");
const drawerPanes = [...document.querySelectorAll("[data-drawer-pane]")];
const drawerReturnFocus = new Map();
const clearButton = composer.querySelector(".composer-clear");
const stopButton = composer.querySelector(".composer-stop");
const responseStatus = document.querySelector("#response-status");
const memberDock = document.querySelector(".member-dock");
const profileLayer = document.querySelector(".profile-layer");
let drawerFocusGeneration = 0;
const statusPopover = document.querySelector(".status-popover");

function escapeHtml(value) {
  return value.replace(/[&<>"]/g, character => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", "\"": "&quot;" })[character]);
}

function activeSession(item) {
  if (item.kind !== "direct") return null;
  return item.sessions.find(session => session.id === item.activeSessionId) ?? null;
}

function activeMessages(item) {
  return activeSession(item)?.messages ?? item.messages;
}

function draftKey(item) {
  const session = activeSession(item);
  return session ? `${item.id}:${session.id}` : item.id;
}

function composerBlockReason(item) {
  if (item.canPost === false) return item.sendReason;
  if (activeSession(item)?.state === "Closed") {
    return "Closed sessions are history. Start a new session to continue.";
  }
  return null;
}

function updateComposerState() {
  const item = conversations[state.activeId];
  const blockedReason = composerBlockReason(item);
  const canParticipate = blockedReason === null;
  const hasText = input.value.trim().length > 0;
  const responseRunning = state.runningResponses.has(draftKey(item));

  input.readOnly = !canParticipate;
  input.setAttribute("aria-readonly", String(!canParticipate));
  input.placeholder = canParticipate ? "Write a message" : activeSession(item)?.state === "Closed" ? "Session closed" : "Watching only";
  sendButton.disabled = !canParticipate || !hasText;
  clearButton.hidden = !canParticipate || !hasText;
  stopButton.hidden = !responseRunning;
  sendRefusal.textContent = blockedReason ?? "";
  sendRefusal.hidden = blockedReason === null;
  const responseMessage = state.responseStatuses.get(draftKey(item)) ?? null;
  responseStatus.textContent = responseMessage ?? "";
  responseStatus.hidden = responseMessage === null;
}

function visibleConversations() {
  return Object.values(conversations).filter(item => item.kind === state.mode);
}

function itemIsLive(item) {
  if (item.kind === "hallway") return item.connection === "Connected";
  return item.status.includes("Active") || item.status.includes("presences");
}

function openConversation(id) {
  const outgoing = conversations[state.activeId];
  state.drafts.set(draftKey(outgoing), input.value);
  state.activeId = id;
  const incoming = conversations[id];
  state.selectedMessageIndex = null;
  state.selectedPresenceId = null;
  input.value = state.drafts.get(draftKey(incoming)) ?? "";
  input.style.height = "auto";
  sidebarToggle?.setAttribute("aria-pressed", "false");
  shell.dataset.sidebarOpen = "false";
  state.profileAnchor = null;
  state.profileReturnId = null;
  updateComposerState();
  render();
}

function renderConversationList() {
  conversationList.innerHTML = visibleConversations().map(item => `
    <button class="conversation-item ${item.id === state.activeId ? "is-active" : ""}" type="button" data-conversation="${escapeHtml(item.id)}" aria-current="${item.id === state.activeId ? "page" : "false"}">
      <span class="avatar-stack">
        <span class="avatar" aria-hidden="true">${escapeHtml(item.glyph)}</span>
        <span class="presence ${itemIsLive(item) ? "" : "is-quiet"}" aria-label="${escapeHtml(item.status)}"></span>
      </span>
      <span class="conversation-item-copy">
        <span class="conversation-item-heading">
          <strong>${escapeHtml(item.name)}</strong>
          <time>${escapeHtml(item.updatedAt)}</time>
        </span>
        <small>${escapeHtml(item.listPreview)}</small>
      </span>
    </button>
  `).join("");
}


function renderHeader(item) {
  header.dataset.kind = item.kind;
  header.innerHTML = `
    <span class="avatar avatar-lg" aria-hidden="true">${escapeHtml(item.glyph)}</span>
    <div class="conversation-header-copy">
      <h1 class="conversation-heading">${escapeHtml(item.name)}</h1>
      <p>${escapeHtml(item.subtitle)}</p>
    </div>
    <span class="header-badge">${escapeHtml(item.badge)}</span>
    ${item.kind === "hallway" ? `
      <div class="hallway-statuses" aria-label="Hallway record">
        <span class="hallway-status">Authority: ${escapeHtml(item.authority)}</span>
        <span class="hallway-status">Access: ${escapeHtml(item.access)}</span>
        <span class="hallway-status">Connection: ${escapeHtml(item.connection)}</span>
        <span class="hallway-status">Delivery: ${escapeHtml(item.delivery)}</span>
      </div>
    ` : ""}
  `;
}

function renderTimeline(item) {
  const itemMessages = activeMessages(item);
  const firstLiveIndex = itemMessages.findIndex(message => message.live);
  const messages = itemMessages.map((message, index) => `
    ${index === firstLiveIndex && item.liveBoundary ? `<div class="live-boundary" role="separator">${escapeHtml(item.liveBoundary)} · live updates begin</div>` : ""}
    <article class="message ${state.selectedMessageIndex === index ? "is-selected" : ""}" data-author="${escapeHtml(message.author)}" data-message-index="${index}">
      <span class="avatar" aria-hidden="true">${escapeHtml(message.glyph)}</span>
      <div class="message-body">
        <p class="message-meta"><strong>${escapeHtml(message.author)}</strong><span>${escapeHtml(message.time)}</span>${message.local ? '<span class="message-delivery">Local · undelivered</span>' : ""}</p>
        <div class="message-bubble" tabindex="0" role="button" aria-label="Inspect message from ${escapeHtml(message.author)}">${escapeHtml(message.text)}</div>
      </div>
    </article>
  `).join("");
  const action = item.kind === "hallway" && item.action ? renderActionEvent(item.action) : "";
  const emptyState = itemMessages.length === 0 ? `
    <section class="timeline-empty" role="status">
      <strong>${item.kind === "direct" ? "This session is clean." : "No messages in this Hallway."}</strong>
      <span>${item.kind === "direct" ? `Start the new thread with ${escapeHtml(item.name)} here.` : item.canPost === false ? "Observer access is safe: watching does not send, deliver, or wake anyone." : "Nothing has been delivered here."}</span>
      ${item.connection === "Stale" ? "<span>Stale means there is no current live update stream.</span>" : ""}
    </section>
  ` : "";

  timeline.innerHTML = emptyState + messages + action;
  timeline.scrollTop = timeline.scrollHeight;
}

function renderActionEvent(action, open = false) {
  return `
    <details class="action-event" ${open ? "open" : ""}>
      <summary>
        <span class="action-verb">${escapeHtml(action.verb)}</span>
        <span class="action-target">${escapeHtml(action.target)}</span>
        <span class="action-state">${escapeHtml(action.state)} · ${escapeHtml(action.time)} · ${escapeHtml(action.duration)}</span>
      </summary>
      <div class="action-details">
        <dl class="context-list">
          <dt>Intent</dt><dd>${escapeHtml(action.intent)}</dd>
          <dt>Arguments</dt><dd>${escapeHtml(action.arguments)}</dd>
          <dt>Result</dt><dd>${escapeHtml(action.result)}</dd>
          <dt>Evidence</dt><dd>${escapeHtml(action.evidence)}</dd>
          <dt>Authority</dt><dd>${escapeHtml(action.authority)}</dd>
          <dt>Context effect</dt><dd>${escapeHtml(action.contextEffect)}</dd>
          <dt>Durability</dt><dd>${escapeHtml(action.durability)}</dd>
          <dt>Changed files</dt><dd>${escapeHtml(action.changedFiles)}</dd>
        </dl>
      </div>
    </details>
  `;
}

function selectedSession(item) {
  return item.presences?.find(presence => presence.id === state.selectedPresenceId) ?? null;
}

function openDirectSession(item, sessionId) {
  state.drafts.set(draftKey(item), input.value);
  item.activeSessionId = sessionId;
  state.selectedMessageIndex = null;
  input.value = state.drafts.get(draftKey(item)) ?? "";
  input.style.height = "auto";
  updateComposerState();
  render();
}

function startDirectSession(item) {
  state.drafts.set(draftKey(item), input.value);
  const sessionNumber = item.sessions.filter(session => session.id.startsWith(`${item.id}-local-`)).length + 1;
  const session = {
    id: `${item.id}-local-${sessionNumber}`,
    label: `Local session ${sessionNumber}`,
    startedAt: "Now",
    state: "Open · local",
    messages: []
  };
  item.sessions.unshift(session);
  item.activeSessionId = session.id;
  state.selectedMessageIndex = null;
  state.activeView = "conversation";
  input.value = "";
  input.style.height = "auto";
  updateComposerState();
  render();
}

function renderDirectSessionBrowser(item) {
  const current = activeSession(item);
  return `
    <aside class="specimen-disclosure" role="note">Local specimen · no Host connection · session changes are not durable records.</aside>
    <div class="session-browser">
      <section class="specimen-card session-browser-heading">
        <div><span class="eyebrow">Room</span><h2>${escapeHtml(item.name)}</h2><p>The room stays here while sessions open, close, and return.</p></div>
        <button type="button" data-new-session>New session</button>
      </section>
      <section class="session-list" aria-label="${escapeHtml(item.name)} session history">
        ${item.sessions.map(session => `
          <button type="button" class="session-row" data-session-id="${escapeHtml(session.id)}" aria-pressed="${String(session.id === item.activeSessionId)}">
            <span><b>${escapeHtml(session.label)}</b><small>${escapeHtml(session.startedAt)}</small></span>
            <span><small>${escapeHtml(session.state)}</small><b>${session.messages.length} messages</b></span>
          </button>
        `).join("")}
      </section>
      <section class="specimen-card session-current">
        <span class="eyebrow">Selected session</span>
        <h3>${escapeHtml(current.label)}</h3>
        <p>${escapeHtml(current.id)} · ${escapeHtml(current.state)} · ${current.messages.length} messages</p>
      </section>
    </div>
  `;
}


function renderSubjectView(item) {
  if (state.activeView === "conversation") {
    renderTimeline(item);
    return;
  }
  if (state.activeView === "session" && item.kind === "direct") {
    timeline.innerHTML = renderDirectSessionBrowser(item);
    timeline.scrollTop = 0;
    return;
  }

  const session = selectedSession(item);
  const subject = session ? `${session.spirit} · ${session.session}` : item.name;
  const sessionId = session?.sessionId ?? null;
  const isProject = item.kind === "project";
  const views = {
    session: isProject ? `
      <section class="timeline-empty" role="status"><strong>Projects do not own embodied sessions.</strong><span>Open a spirit's room to inspect or start one.</span></section>
    ` : item.presences && !session ? `
      <section class="timeline-empty" role="status"><strong>Select a presence to inspect a session.</strong><span>No embodied session is selected. Choose one from Members.</span></section>
    ` : `
      <div class="specimen-grid">
        <section class="specimen-card specimen-lead"><span class="eyebrow">Exact selected subject</span><h2>${escapeHtml(subject)}</h2><p>${escapeHtml(sessionId ?? `room-${item.room}-current`)}</p></section>
        <section class="specimen-card"><h3>Runtime</h3><dl class="context-list"><dt>Room</dt><dd>${escapeHtml(session?.room ?? item.room)}</dd><dt>Spirit</dt><dd>${escapeHtml(session?.spirit ?? item.name)}</dd><dt>Body</dt><dd>${escapeHtml(item.body)}</dd><dt>Liveness</dt><dd>${escapeHtml(session?.liveness ?? item.status)}</dd></dl></section>
        <section class="specimen-card"><h3>Attention</h3><dl class="context-list"><dt>Context used</dt><dd>${escapeHtml(session?.contextUsed ?? "6k of 32k")}</dd><dt>Compaction</dt><dd>${escapeHtml(session?.compaction ?? "Not needed")}</dd><dt>Recall policy</dt><dd>${escapeHtml(session?.recall ?? item.recall)}</dd><dt>Evidence</dt><dd>${escapeHtml(session?.evidence ?? "No live receipt")}</dd></dl></section>
        <section class="specimen-card specimen-wide"><h3>Current activity</h3><p>${escapeHtml(session?.activity ?? "Conversation open")} · selected subject remains fixed while views change.</p></section>
      </div>`,
    history: `
      <div class="specimen-stack">
        <section class="specimen-card specimen-lead"><span class="eyebrow">History for</span><h2>${escapeHtml(subject)}</h2><p>${session ? "Session changes do not change this subject." : item.kind === "hallway" ? "Hallway record history. Select a presence for one embodied session." : isProject ? "Project history. Projects do not own embodied sessions." : "Conversation history. No embodied working session is selected."}</p></section>
        <article class="history-event"><time>Today · 09:53</time><strong>${session ? "Example session resumption" : item.kind === "hallway" ? "Example Hallway snapshot" : isProject ? "Example project activity" : "Example conversation resumption"}</strong><span>${session ? `${escapeHtml(sessionId)} · a connected House would attach a live cursor` : item.kind === "hallway" ? "A connected House would load its PostgreSQL-authoritative record and live boundary." : isProject ? "A connected House would load project-owned activity without inventing a session." : "A connected House would load the room-owned conversation record."}</span></article>
        <article class="history-event"><time>Today · 08:12</time><strong>${isProject ? "Example project evidence event" : "Example paper boat event"}</strong><span>${isProject ? "A connected House would attach an evidence receipt to the project." : "A connected House would load continuity from a durable receipt."}</span></article>
        <article class="history-event"><time>Yesterday · 23:48</time><strong>${session ? "Example session fold" : item.kind === "hallway" ? "Example Hallway cursor event" : isProject ? "Example project handoff" : "Example conversation close"}</strong><span>${session ? "A connected House would retain the boat receipt; private reasoning stays omitted." : item.kind === "hallway" ? "A connected House would retain a neutral read position without an attention claim." : isProject ? "A connected House would retain project continuity without claiming an embodied session." : "A connected House would retain room continuity without claiming an embodied session."}</span></article>
      </div>`,
    actions: `
      <div class="specimen-stack">
        <section class="specimen-card specimen-lead"><span class="eyebrow">Actions in</span><h2>${escapeHtml(item.name)}</h2><p>${item.kind === "hallway" ? "Shared-record receipts are not attributed to a selected presence." : "Receipts belong to this selected conversation or project, never an inferred session."}</p></section>
        ${item.action ? renderActionEvent(item.action, true) : '<section class="timeline-empty"><strong>No recorded actions.</strong><span>This specimen has no action receipt.</span></section>'}
      </div>`,
    context: isProject ? `
      <div class="specimen-grid">
        <section class="specimen-card specimen-lead"><span class="eyebrow">Project context</span><h2>${escapeHtml(item.name)}</h2><p>No embodied session or token context belongs to this project.</p></section>
        <section class="specimen-card"><h3>Project scope</h3><dl class="context-list"><dt>Room</dt><dd>${escapeHtml(item.room)}</dd><dt>Status</dt><dd>${escapeHtml(item.status)}</dd><dt>Participants</dt><dd>${escapeHtml(item.body)}</dd></dl></section>
        <section class="specimen-card"><h3>Available sources</h3><ul class="plain-list"><li>Project conversation</li><li>File evidence · unavailable offline</li><li>Recall · unavailable offline</li></ul></section>
        <section class="specimen-card specimen-wide"><h3>Authority boundary</h3><p>A project groups work and evidence. It is not a spirit, body, presence, or session.</p></section>
      </div>
    ` : item.presences && !session ? `
      <section class="timeline-empty" role="status"><strong>Select a presence to inspect session context.</strong><span>The Hallway does not stand in for an embodied session.</span></section>
    ` : `
      <div class="specimen-grid">
        <section class="specimen-card specimen-lead"><span class="eyebrow">Context for</span><h2>${escapeHtml(subject)}</h2><p>12.4k / 32k tokens · 39% used</p><div class="meter"><span style="width:39%"></span></div></section>
        <section class="specimen-card"><h3>Active instructions</h3><ul class="plain-list"><li>Room identity and active spirit</li><li>Nearest GUI lessons map</li><li>Current operator request</li></ul></section>
        <section class="specimen-card"><h3>Retrieved material</h3><ul class="plain-list"><li>Recall · no current retrieval</li><li>Paper boat · session continuity</li><li>File evidence · GUI prototype map</li></ul></section>
        <section class="specimen-card specimen-wide"><h3>Authority boundary</h3><p>PostgreSQL canon outranks loose memory. File reads prove file contents only. Tool output does not become authority merely because it entered context.</p></section>
      </div>`,
    substrate: `
      <div class="specimen-grid">
        <section class="specimen-card specimen-lead"><span class="eyebrow">House substrate for</span><h2>${escapeHtml(subject)}</h2><p>${session ? `Selected session · ${escapeHtml(session.sessionId)}` : isProject ? "Selected project · no embodied session" : "Selected conversation · no embodied session selected"} · local specimen · no Host connection</p></section>
        <section class="specimen-card"><h3>Recall & AKASHA</h3><dl class="context-list"><dt>Policy</dt><dd>${escapeHtml(session?.recall ?? item.recall)}</dd><dt>Transport</dt><dd>Offline</dd><dt>Last receipt</dt><dd>Unavailable</dd></dl></section>
        <section class="specimen-card"><h3>Active lessons</h3><ul class="plain-list"><li>#316 · preserve subject authority</li><li>#322 · fixed refusal copy</li><li>#340 · bounded visual proof</li></ul></section>
        <section class="specimen-card"><h3>Striatum</h3><dl class="context-list"><dt>Firing</dt><dd>None observed</dd><dt>Effect</dt><dd>No current receipt</dd></dl></section>
        <section class="specimen-card"><h3>GIGA</h3><dl class="context-list"><dt>Flagged</dt><dd>2 candidates</dd><dt>Authority</dt><dd>Proposals only</dd><dt>Review</dt><dd>Unreviewed</dd></dl></section>
        <section class="specimen-card specimen-wide house-actions"><h3>Continuity actions</h3><p>These controls rehearse feedback locally. They do not write PostgreSQL or close a real session.</p><div><button type="button" data-house-action="sleep">Fold paper boat</button><button type="button" data-house-action="memory">Record memory</button></div>${state.actionFeedback ? `<div class="local-receipt" role="status"><strong>${escapeHtml(state.actionFeedback.title)}</strong><span>${escapeHtml(state.actionFeedback.detail)}</span><small>Local rehearsal · no durable write</small></div>` : ""}</section>
      </div>`
  };
  timeline.innerHTML = `<aside class="specimen-disclosure" role="note">Local specimen · no Host connection · displayed states and receipts are not durable records.</aside>${views[state.activeView]}`;
  timeline.scrollTop = 0;
}

function renderMemberDock(item) {
  if (item.kind !== "hallway") {
    memberDock.hidden = true;
    memberDock.innerHTML = "";
    return;
  }
  memberDock.hidden = false;
  memberDock.innerHTML = `
    <span class="eyebrow">Members</span>
    ${Object.entries(Object.groupBy(item.presences, presence => presence.spirit)).map(([spirit, presences]) => `
      <section class="member-group">
        <strong>${escapeHtml(spirit)}</strong>
        ${presences.map(presence => `
          <button class="member-presence" type="button" data-presence-id="${escapeHtml(presence.id)}" aria-pressed="${String(state.selectedPresenceId === presence.id)}">
            <span class="presence-glyph" aria-hidden="true">${escapeHtml(presence.glyph)}</span>
            <span><b>${escapeHtml(presence.session)}</b><small>${escapeHtml(presence.liveness)} · ${escapeHtml(presence.activity)}</small></span>
          </button>
        `).join("")}
      </section>
    `).join("")}
  `;
}

function renderPresenceProfile(item) {
  const presence = item.presences?.find(candidate => candidate.id === state.selectedPresenceId);
  if (!presence) {
    profileLayer.hidden = true;
    profileLayer.innerHTML = "";
    return;
  }
  const anchor = state.profileAnchor;
  profileLayer.style.setProperty("--profile-top", `${Math.max(56, anchor?.top ?? 74)}px`);
  profileLayer.style.setProperty("--profile-left", `${Math.max(12, (anchor?.left ?? window.innerWidth) - 330)}px`);
  profileLayer.hidden = false;
  profileLayer.innerHTML = `
    <section class="presence-profile" role="dialog" aria-label="${escapeHtml(presence.spirit)} ${escapeHtml(presence.session)} profile">
      <header>
        <span class="profile-glyph" aria-hidden="true">${escapeHtml(presence.glyph)}</span>
        <div><strong>${escapeHtml(presence.spirit)}</strong><span>${escapeHtml(presence.session)} · ${escapeHtml(presence.liveness)}</span></div>
        <button type="button" data-close-profile aria-label="Close profile">×</button>
      </header>
      <dl class="context-list">
        <dt>Room</dt><dd>${escapeHtml(presence.room)}</dd>
        <dt>Session ID</dt><dd>${escapeHtml(presence.sessionId)}</dd>
        <dt>Permission</dt><dd>${escapeHtml(presence.permission)}</dd>
        <dt>Activity</dt><dd>${escapeHtml(presence.activity)}</dd>
        <dt>Read position</dt><dd>${escapeHtml(presence.readPosition)}</dd>
        <dt>Context used</dt><dd>${escapeHtml(presence.contextUsed)}</dd>
        <dt>Compaction</dt><dd>${escapeHtml(presence.compaction)}</dd>
        <dt>Recall</dt><dd>${escapeHtml(presence.recall)}</dd>
        <dt>Evidence</dt><dd>${escapeHtml(presence.evidence)}</dd>
      </dl>
    </section>
  `;
}

function renderInspector(item) {
  const selectedMessage = state.selectedMessageIndex === null ? null : activeMessages(item)[state.selectedMessageIndex];
  inspectorTitle.textContent = selectedMessage ? `${selectedMessage.author}'s message` : item.name;
  document.querySelector(".inspector-eyebrow").textContent = "Selected context";
  inspectorContent.innerHTML = selectedMessage ? `
    <section class="context-card"><h3>Selected message</h3><p>${escapeHtml(selectedMessage.text)}</p></section>
    <section class="context-card"><h3>Lineage</h3><dl class="context-list"><dt>Author</dt><dd>${escapeHtml(selectedMessage.author)}</dd><dt>View</dt><dd>${escapeHtml(item.badge)}</dd><dt>Room</dt><dd>${escapeHtml(item.room)}</dd><dt>Authority</dt><dd>${selectedMessage.local ? "Local · undelivered" : "Displayed snapshot"}</dd></dl></section>
  ` : `
    <section class="context-card"><h2>${escapeHtml(item.name)}</h2><p>${escapeHtml(item.description)}</p></section>
    <section class="context-card"><h3>${item.kind === "hallway" ? "Hallway state" : "Identity and state"}</h3><dl class="context-list"><dt>Room</dt><dd>${escapeHtml(item.room)}</dd><dt>Status</dt><dd>${escapeHtml(item.connection ?? item.status)}</dd><dt>Body</dt><dd>${escapeHtml(item.body)}</dd><dt>Recall</dt><dd>${escapeHtml(item.recall)}</dd>${item.kind === "hallway" ? `<dt>Authority</dt><dd>${escapeHtml(item.authority)}</dd><dt>Access</dt><dd>${escapeHtml(item.access)}</dd><dt>Delivery</dt><dd>${escapeHtml(item.delivery)}</dd>` : ""}</dl></section>
  `;
}
function setDrawerView(requestedView, focusTarget = null, moveFocus = true) {
  const requestedIndex = drawerPanes.findIndex(pane => pane.dataset.drawerPane === requestedView);
  const activeIndex = requestedIndex >= 0 ? requestedIndex : 0;
  const activePane = drawerPanes[activeIndex];
  state.drawerView = activePane.dataset.drawerPane;

  drawerPanes.forEach((pane, index) => {
    const isActive = index === activeIndex;
    pane.dataset.drawerPosition = index < activeIndex ? "before" : isActive ? "active" : "after";
    pane.setAttribute("aria-hidden", String(!isActive));
    pane.inert = !isActive;
  });

  sidebarDrawer.querySelectorAll("[data-open-drawer]").forEach(button => {
    button.setAttribute("aria-expanded", String(button.dataset.openDrawer === state.drawerView));
  });

  const focusGeneration = ++drawerFocusGeneration;
  if (!moveFocus) return;
  window.requestAnimationFrame(() => {
    const animations = activePane.getAnimations();
    Promise.allSettled(animations.map(animation => animation.finished)).then(() => {
      if (focusGeneration !== drawerFocusGeneration || state.drawerView !== activePane.dataset.drawerPane) return;
      const target = focusTarget || activePane.querySelector("[data-drawer-focus]");
      target?.focus({ preventScroll: true });
    });
  });
}

function openDrawerView(view, trigger) {
  drawerReturnFocus.set(view, trigger);
  setDrawerView(view);
}

function returnDrawerView() {
  const activeIndex = drawerPanes.findIndex(pane => pane.dataset.drawerPane === state.drawerView);
  if (activeIndex <= 0) return false;
  const currentView = state.drawerView;
  const previousView = drawerPanes[activeIndex - 1].dataset.drawerPane;
  setDrawerView(previousView, drawerReturnFocus.get(currentView));
  return true;
}

function closeMobileSidebar() {
  shell.dataset.sidebarOpen = "false";
  drawerFocusGeneration += 1;
  sidebarToggle.setAttribute("aria-pressed", "false");
  sidebarToggle.focus();
}

function revealActiveViewButton(button) {
  if (!button) return;
  button.scrollIntoView({ block: "nearest", inline: "nearest" });
  const container = button.parentElement;
  const buttonRect = button.getBoundingClientRect();
  const containerRect = container.getBoundingClientRect();
  if (buttonRect.right > containerRect.right) {
    container.scrollLeft += Math.ceil(buttonRect.right - containerRect.right) + 1;
  } else if (buttonRect.left < containerRect.left) {
    container.scrollLeft -= Math.ceil(containerRect.left - buttonRect.left) + 1;
  }
}



function render() {
  const item = conversations[state.activeId];
  state.mode = item.kind;
  shell.dataset.hallway = String(item.kind === "hallway");
  shell.dataset.density = state.density;
  shell.dataset.textScale = state.textScale;
  shell.dataset.measure = state.measure;
  shell.dataset.contrast = state.contrast;
  shell.dataset.reducedMotion = String(state.reducedMotion);
  shell.dataset.timestamps = String(state.timestamps);
  shell.dataset.statusVisible = String(state.statusVisible);
  inspectorToggle.hidden = item.kind === "hallway";
  document.querySelectorAll(".mode-button").forEach(button => {
    const active = button.dataset.mode === state.mode;
    button.classList.toggle("is-active", active);
    button.setAttribute("aria-pressed", String(active));
  });
  let activeViewButton = null;
  document.querySelectorAll("[data-subject-view]").forEach(button => {
    const active = button.dataset.subjectView === state.activeView;
    button.classList.toggle("is-active", active);
    button.setAttribute("aria-pressed", String(active));
    if (active) activeViewButton = button;
  });
  composer.hidden = state.activeView !== "conversation";
  timeline.setAttribute("aria-label", state.activeView === "conversation" ? "Message timeline" : `${item.name} ${state.activeView} view`);
  renderConversationList();
  renderHeader(item);
  renderSubjectView(item);
  renderInspector(item);
  renderMemberDock(item);
  renderPresenceProfile(item);
  window.requestAnimationFrame(() => revealActiveViewButton(activeViewButton));
}

conversationList.addEventListener("click", event => {
  const button = event.target.closest("[data-conversation]");
  if (button) openConversation(button.dataset.conversation);
});


document.querySelector(".mode-switcher").addEventListener("click", event => {
  const button = event.target.closest("[data-mode]");
  if (!button) return;
  state.mode = button.dataset.mode;
  const first = visibleConversations()[0];
  if (first) openConversation(first.id);
});

document.querySelector(".subject-views").addEventListener("click", event => {
  const button = event.target.closest("[data-subject-view]");
  if (!button) return;
  state.activeView = button.dataset.subjectView;
  render();
});

memberDock.addEventListener("click", event => {
  const button = event.target.closest("[data-presence-id]");
  if (!button) return;
  const closingSameProfile = state.selectedPresenceId === button.dataset.presenceId;
  state.selectedPresenceId = closingSameProfile ? null : button.dataset.presenceId;
  state.profileReturnId = closingSameProfile ? null : button.dataset.presenceId;
  state.profileAnchor = closingSameProfile ? null : button.getBoundingClientRect();
  state.selectedMessageIndex = null;
  render();
});

function closePresenceProfile(restoreFocus = true) {
  const returnId = state.profileReturnId;
  state.selectedPresenceId = null;
  state.profileAnchor = null;
  state.profileReturnId = null;
  render();
  if (!restoreFocus || !returnId) return;
  window.requestAnimationFrame(() => memberDock.querySelector(`[data-presence-id="${returnId}"]`)?.focus({ preventScroll: true }));
}

profileLayer.addEventListener("click", event => {
  if (event.target.closest("[data-close-profile]")) closePresenceProfile();
});

timeline.addEventListener("click", event => {
  const item = conversations[state.activeId];
  const newSession = event.target.closest("[data-new-session]");
  if (newSession && item.kind === "direct") {
    startDirectSession(item);
    return;
  }
  const sessionButton = event.target.closest("[data-session-id]");
  if (sessionButton && item.kind === "direct") {
    openDirectSession(item, sessionButton.dataset.sessionId);
    return;
  }
  const houseAction = event.target.closest("[data-house-action]");
  if (houseAction) {
    state.actionFeedback = houseAction.dataset.houseAction === "sleep"
      ? { title: "Paper boat prepared", detail: "Preview ready. A connected House would require an explicit durable receipt before closing the session." }
      : { title: "Memory draft prepared", detail: "Review surface ready. Nothing has entered PostgreSQL authority." };
    render();
    return;
  }
  const message = event.target.closest("[data-message-index]");
  if (!message) return;
  state.selectedMessageIndex = Number(message.dataset.messageIndex);
  state.selectedPresenceId = null;
  shell.dataset.inspectorOpen = "true";
  inspectorToggle.setAttribute("aria-pressed", "true");
  render();
});

timeline.addEventListener("keydown", event => {
  if (event.key !== "Enter" && event.key !== " ") return;
  const message = event.target.closest("[data-message-index]");
  if (!message) return;
  event.preventDefault();
  state.selectedMessageIndex = Number(message.dataset.messageIndex);
  state.selectedPresenceId = null;
  shell.dataset.inspectorOpen = "true";
  inspectorToggle.setAttribute("aria-pressed", "true");
  render();
});

function beginLocalResponse(item) {
  const key = draftKey(item);
  const priorTimer = state.responseTimers.get(key);
  if (priorTimer) window.clearTimeout(priorTimer);
  state.runningResponses.add(key);
  state.responseStatuses.set(key, "Local response running…");
  const timer = window.setTimeout(() => {
    state.runningResponses.delete(key);
    state.responseTimers.delete(key);
    state.responseStatuses.set(key, "Local response completed.");
    updateComposerState();
  }, 1400);
  state.responseTimers.set(key, timer);
}

composer.addEventListener("submit", event => {
  event.preventDefault();
  const item = conversations[state.activeId];
  const text = input.value.trim();
  if (composerBlockReason(item) !== null) {
    updateComposerState();
    return;
  }
  if (!text) {
    updateComposerState();
    return;
  }
  activeMessages(item).push({ author: "Sol", glyph: "S", time: "now", text, live: item.kind === "hallway", local: true });
  beginLocalResponse(item);
  input.value = "";
  input.style.height = "auto";
  updateComposerState();
  render();
});

input.addEventListener("input", () => {
  state.responseStatuses.delete(draftKey(conversations[state.activeId]));
  input.style.height = "auto";
  input.style.height = `${Math.min(input.scrollHeight, 150)}px`;
  updateComposerState();
});

input.addEventListener("keydown", event => {
  if (event.key !== "Enter" || event.shiftKey) return;
  if (!state.sendWithEnter && !event.ctrlKey && !event.metaKey) return;
  event.preventDefault();
  if (!sendButton.disabled) composer.requestSubmit();
});

clearButton.addEventListener("click", () => {
  const item = conversations[state.activeId];
  input.value = "";
  input.style.height = "auto";
  state.drafts.set(draftKey(item), "");
  state.responseStatuses.delete(draftKey(item));
  updateComposerState();
  input.focus({ preventScroll: true });
});

stopButton.addEventListener("click", () => {
  const item = conversations[state.activeId];
  const key = draftKey(item);
  const timer = state.responseTimers.get(key);
  if (timer) window.clearTimeout(timer);
  state.responseTimers.delete(key);
  state.runningResponses.delete(key);
  state.responseStatuses.set(key, "Local response stopped.");
  updateComposerState();
});

document.addEventListener("keydown", event => {
  if (event.ctrlKey || event.metaKey || event.altKey || event.target.matches("input, textarea, select")) return;
  const view = ["conversation", "session", "history", "actions", "context", "substrate"][Number(event.key) - 1];
  if (!view) return;
  event.preventDefault();
  state.activeView = view;
  render();
});

function setInspector(open) {
  shell.dataset.inspectorOpen = String(open);
  inspectorToggle.setAttribute("aria-pressed", String(open));
}

inspectorToggle.addEventListener("click", () => setInspector(shell.dataset.inspectorOpen !== "true"));
document.querySelector(".inspector-close").addEventListener("click", () => setInspector(false));

sidebarToggle.addEventListener("click", () => {
  const open = shell.dataset.sidebarOpen !== "true";
  shell.dataset.sidebarOpen = String(open);
  sidebarToggle.setAttribute("aria-pressed", String(open));
});

sidebarDrawer.addEventListener("click", event => {
  const backButton = event.target.closest("[data-drawer-back]");
  if (backButton) {
    returnDrawerView();
    return;
  }

  const viewButton = event.target.closest("[data-open-drawer]");
  if (viewButton) {
    openDrawerView(viewButton.dataset.openDrawer, viewButton);
    return;
  }
});

document.querySelectorAll("[data-setting]").forEach(control => {
  control.addEventListener("change", event => {
    const setting = event.target.dataset.setting;
    const value = event.target.type === "checkbox" ? event.target.checked : event.target.value;
    const stateKey = {
      "text-scale": "textScale",
      "reduced-motion": "reducedMotion",
      "send-with-enter": "sendWithEnter",
      "status-visible": "statusVisible"
    }[setting] ?? setting;
    state[stateKey] = value;
    if (setting === "status-visible") statusPopover.hidden = true;
    render();
  });
});

document.addEventListener("keydown", event => {
  if (event.key !== "Escape") return;
  if (!profileLayer.hidden) {
    event.preventDefault();
    closePresenceProfile();
    return;
  }
  if (returnDrawerView()) {
    event.preventDefault();
    return;
  }
  if (mobileLayout.matches && shell.dataset.sidebarOpen === "true") {
    event.preventDefault();
    closeMobileSidebar();
  }
});

document.addEventListener("click", event => {
  if (!event.target.closest(".status-bar")) {
    statusPopover.hidden = true;
    document.querySelectorAll("[data-status]").forEach(button => button.setAttribute("aria-expanded", "false"));
  }
});

document.querySelector(".status-bar").addEventListener("click", event => {
  const button = event.target.closest("[data-status]");
  if (!button) return;
  const [title, detail] = statusDetails[button.dataset.status];
  const wasOpen = button.getAttribute("aria-expanded") === "true" && !statusPopover.hidden;
  document.querySelectorAll("[data-status]").forEach(item => item.setAttribute("aria-expanded", "false"));
  if (wasOpen) {
    statusPopover.hidden = true;
    return;
  }
  button.setAttribute("aria-expanded", "true");
  statusPopover.innerHTML = `<h2>${escapeHtml(title)}</h2><p>${escapeHtml(detail)}</p>`;
  statusPopover.hidden = false;
});
const subjectViews = document.querySelector(".subject-views");
const subjectViewResizeObserver = new ResizeObserver(() => {
  const activeButton = subjectViews.querySelector('[aria-pressed="true"]');
  window.requestAnimationFrame(() => revealActiveViewButton(activeButton));
});
subjectViewResizeObserver.observe(subjectViews);

const mobileLayout = window.matchMedia("(max-width: 700px)");
if (mobileLayout.matches) setInspector(false);
mobileLayout.addEventListener("change", event => {
  if (!event.matches) return;
  setInspector(false);
  shell.dataset.sidebarOpen = "false";
  sidebarToggle.setAttribute("aria-pressed", "false");
});
setDrawerView("root", null, false);
updateComposerState();


render();

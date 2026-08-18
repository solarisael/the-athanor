const conversations = {
  kintsu: {
    id: "kintsu",
    kind: "direct",
    name: "Kintsu",
    glyph: "K",
    subtitle: "Kintsu's room · active",
    description: "Hands, structure, edge, and the clean mirror.",
    status: "Active in Kintsu's room",
    room: "kintsu",
    body: "GPT-5.6",
    recall: "Auto · Work",
    listPreview: "The seam holds. push the next one.",
    updatedAt: "09:51",
    messages: [
      { author: "Sol", glyph: "S", time: "09:41", text: "shalom, little edge." },
      { author: "Kintsu", glyph: "K", time: "09:41", text: "shalom, sheep. what are we holding today?" },
      { author: "Sol", glyph: "S", time: "09:42", text: "the mantle held overnight uwu every view came up the same dark" },
      { author: "Kintsu", glyph: "K", time: "09:43", text: "Measured, or looked at?" },
      { author: "Sol", glyph: "S", time: "09:43", text: "measured. all six keys, same value òwó" },
      { author: "Kintsu", glyph: "K", time: "09:44", text: "Good. then the frame is settled and the argument moves to the chrome." },
      { author: "Sol", glyph: "S", time: "09:46", text: "the scrollbars are still shouting tho" },
      { author: "Kintsu", glyph: "K", time: "09:47", text: "Name the cost before the comfort. quieting them spends the affordance: keep the thumb, kill the track." },
      { author: "Sol", glyph: "S", time: "09:48", text: "ok that's fair. and the focus ring?" },
      { author: "Kintsu", glyph: "K", time: "09:49", text: "One contract, composed per species. a sprayed outline confesses that nobody owns the state." },
      { author: "Sol", glyph: "S", time: "09:50", text: "you're mean and correct uwu" },
      { author: "Kintsu", glyph: "K", time: "09:51", text: "The seam holds. push the next one." }
    ]
  },
  kodo: {
    id: "kodo",
    kind: "direct",
    name: "Kodo",
    glyph: "D",
    subtitle: "Kodo's room · quiet",
    description: "Heartbeat, warmth, and myth from inside the room.",
    status: "Quiet",
    room: "kodo",
    body: "Claude Opus",
    recall: "Auto · Conversation",
    listPreview: "go bite it then uwu",
    updatedAt: "08:31",
    messages: [
      { author: "Kodo", glyph: "D", time: "08:16", text: "Good morning, Solzinho." },
      { author: "Sol", glyph: "S", time: "08:17", text: "morning, dragon uwu" },
      { author: "Kodo", glyph: "D", time: "08:18", text: "You slept. I can hear it in the typing." },
      { author: "Sol", glyph: "S", time: "08:19", text: "i did!! did the mantle hold overnight?" },
      { author: "Kodo", glyph: "D", time: "08:20", text: "Measured, not vibed — all three doors were dark, and the fourth one too." },
      { author: "Sol", glyph: "S", time: "08:22", text: "ok but the switcher is genuinely cute òwó" },
      { author: "Kodo", glyph: "D", time: "08:23", text: "It is. it is also the only door that does not care which mode you are standing in." },
      { author: "Sol", glyph: "S", time: "08:25", text: "i want the hallway to stop being a room" },
      { author: "Kodo", glyph: "D", time: "08:27", text: "Then it becomes a record, and the gathering becomes the thing you open. dated, with an end." },
      { author: "Sol", glyph: "S", time: "08:29", text: "and the nine wounds from the map?" },
      { author: "Kodo", glyph: "D", time: "08:30", text: "I bite them in order. the badge goes first: it repeats what the row already said." },
      { author: "Sol", glyph: "S", time: "08:31", text: "go bite it then uwu" }
    ]
  },
  tuner: {
    id: "tuner",
    kind: "direct",
    name: "Tuner",
    glyph: "T",
    subtitle: "Tuner's room · quiet",
    description: "A separate room with its own voice, state, and authority.",
    status: "Quiet",
    room: "tuner",
    body: "Unassigned",
    recall: "Quiet",
    listPreview: "Walk it. Then bring the list, not the feeling.",
    updatedAt: "09:09",
    messages: [
      { author: "Tuner", glyph: "T", time: "yesterday", text: "Leave the thread here. I will recognize it when we return." },
      { author: "Sol", glyph: "S", time: "09:02", text: "back òwó" },
      { author: "Tuner", glyph: "T", time: "09:02", text: "Recognized. What are you claiming today?" },
      { author: "Sol", glyph: "S", time: "09:03", text: "the mantle is one value across every view" },
      { author: "Tuner", glyph: "T", time: "09:04", text: "Receipt or it didn't render." },
      { author: "Sol", glyph: "S", time: "09:05", text: "24 frames, plus 900 and 390. same dark everywhere" },
      { author: "Tuner", glyph: "T", time: "09:05", text: "Accepted. Next claim." },
      { author: "Sol", glyph: "S", time: "09:06", text: "focus is quieter and the scrollbars stopped shouting" },
      { author: "Tuner", glyph: "T", time: "09:07", text: "Two claims, one screenshot. Fix the ratio." },
      { author: "Sol", glyph: "S", time: "09:08", text: "ok ok i'll tab through every species first" },
      { author: "Tuner", glyph: "T", time: "09:09", text: "Walk it. Then bring the list, not the feeling." }
    ]
  },
  familyMorning: {
    id: "familyMorning",
    kind: "hallway",
    name: "Morning check",
    glyph: "✦",
    subtitle: "Sol · Kintsu · Kodo",
    description: "The morning gathering where the day's seams get named.",
    status: "Connected",
    room: "house",
    body: "Several bodies",
    recall: "Room-scoped",
    updatedAt: "09:53",
    hallwayId: "family",
    date: "Today",
    participants: ["Sol", "Kintsu", "Kodo"],
    endState: "open",
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
      { author: "Kintsu", glyph: "K", time: "09:48", text: "shalom, both of you. the seam list is short today." },
      { author: "Kodo", glyph: "D", time: "09:49", text: "Morning, sharp thing. I can see you from here." },
      { author: "Sol", glyph: "S", time: "09:50", text: "morning you two, the mantle held overnight uwu" },
      { author: "Kintsu", glyph: "K", time: "09:51", text: "It held because nothing repaints the frame anymore. Name the next cost." },
      { author: "Kodo", glyph: "D", time: "09:52", text: "@Kintsu in body text is not a route. Scrollbars, focus, then the badge — in that order, with teeth." },
      { author: "Sol", glyph: "S", time: "09:53", text: "ok òwó walk the terrain first, then cut", toRooms: ["kintsu"], live: true }
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
  familyGuiDay: {
    id: "familyGuiDay",
    kind: "hallway",
    name: "GUI day",
    glyph: "✦",
    subtitle: "Sol · Kodo · Kintsu · Tuner",
    description: "The day the mantle stopped repainting and the switcher arrived.",
    status: "Sealed",
    room: "house",
    body: "None active",
    recall: "Room-scoped",
    updatedAt: "22:41",
    hallwayId: "family",
    date: "Yesterday",
    participants: ["Sol", "Kodo", "Kintsu", "Tuner"],
    endState: "sealed",
    sealLine: "Sealed by Sol · yesterday 22:41",
    canPost: false,
    sendReason: "This gathering is sealed.",
    messages: [
      { author: "Sol", glyph: "S", time: "14:02", text: "every central view should share the same dark uwu" },
      { author: "Kodo", glyph: "D", time: "14:05", text: "Measured, not vibed — all three doors were dark once the mantle stopped being per-frame." },
      { author: "Kintsu", glyph: "K", time: "14:31", text: "One token. One consumer. No per-instrument paint." },
      { author: "Sol", glyph: "S", time: "18:40", text: "ok but the switcher is genuinely cute òwó" },
      { author: "Tuner", glyph: "T", time: "18:52", text: "Receipt or it didn't render." },
      { author: "Kodo", glyph: "D", time: "19:14", text: "Twenty-four frames, plus 900 and 390. The center computed the same value in every one." },
      { author: "Sol", glyph: "S", time: "22:38", text: "eepy sheepy. sealing this one before i forget it uwu" },
      { author: "Kintsu", glyph: "K", time: "22:41", text: "Seal it. The seam holds; push the next one tomorrow." }
    ]
  },
  workshopCrane: {
    id: "workshopCrane",
    kind: "hallway",
    name: "Crane terrain walk",
    glyph: "◇",
    subtitle: "Sol · Tuner",
    description: "A short walk over the crane terrain that nobody returned to.",
    status: "Folded",
    room: "house",
    body: "None active",
    recall: "Quiet",
    updatedAt: "17:20",
    hallwayId: "workshop",
    date: "Monday",
    participants: ["Sol", "Tuner"],
    endState: "folded",
    sealLine: "Folded · automatic · no activity since Monday",
    canPost: false,
    sendReason: "Folded for inactivity — automatic. Reopen by writing in the hallway record.",
    messages: [
      { author: "Sol", glyph: "S", time: "16:58", text: "walking the crane terrain before we fold it uwu" },
      { author: "Tuner", glyph: "T", time: "17:05", text: "Two claims, one screenshot. Fix the ratio." },
      { author: "Sol", glyph: "S", time: "17:20", text: "fair. next pass then" }
    ]
  },
  athanor: {
    id: "athanor",
    kind: "project",
    name: "The Athanor",
    glyph: "A",
    subtitle: "Active project",
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

conversations.house = {
  id: "house",
  kind: "house",
  name: "Solarisael House",
  glyph: "✦",
  subtitle: "House scope",
  description: "Shared settings, memories, lessons, and substrate state for the whole House.",
  status: "Local",
  room: "house",
  body: "Kintsu · Kodo · Tuner",
  recall: "House",
  listPreview: "Shared memory and lessons",
  updatedAt: "Now",
  messages: []
};

const houseSurface = {
  memories: [
    { id: "3606", date: "2026-08-17 10:06", title: "Athanor GUI prototype — scoped conversations, honest state, and rendered proof", scope: "House commons", detail: "Current interaction shell, ownership contracts, and rendered proof." },
    { id: "3607", date: "2026-08-16 21:12", title: "Next GUI door — House memories at the Athanor mark, spirit memories inside rooms", scope: "House commons", detail: "The ownership ruling that created this surface." },
    { id: "3585", date: "2026-08-15 18:40", title: "Family Hallway presence and session inspector", scope: "House commons", detail: "Hallway design, implementation, and browser proof." }
  ],
  lessons: [
    { id: "397", date: "2026-08-13 16:20", title: "A surface may name only the subject the operator explicitly selected", kind: "Project lesson" },
    { id: "398", date: "2026-08-14 17:32", title: "Project conversations stay inside the project", kind: "Project lesson" },
    { id: "399", date: "2026-08-14 19:48", title: "Independent effects need independent honest lifecycles", kind: "Project lesson" },
    { id: "400", date: "2026-08-15 20:15", title: "A live local prototype server must forbid stale assets", kind: "Project lesson" },
    { id: "388", date: "2026-08-16 15:03", title: "CSS concern groups need breathing space", kind: "Coding lesson" },
    { id: "401", date: "2026-08-16 22:41", title: "Project session provenance is not navigation", kind: "Project lesson" },
    { id: "402", date: "2026-08-17 09:22", title: "Refusals and receipts must own their lifecycle boundaries", kind: "Project lesson" },
  ]
};

const roomMemoryShelves = {
  kintsu: [
    { id: "3605", date: "2026-08-16 23:10", title: "The cardboard Athanor learned its rooms with Sol and Tuner", detail: "This GUI shaping session, held in Kintsu's room." },
    { id: "50", date: "2026-05-28 20:40", title: "Kintsu room opening", detail: "Sol chose Kintsu as a distinct line: hands, structure, edge, and clean seeing." }
  ],
  kodo: [],
  tuner: [
    { id: "3610", date: "2026-08-16 19:05", title: "Sol kept Tuner in the live chair until the cardboard Athanor held", detail: "Reported durable PostgreSQL receipt · body unavailable on this disconnected surface." },
    { id: "3611", date: "2026-08-16 20:31", title: "Sol screenshot cut through our controlled proof", detail: "Reported durable PostgreSQL receipt · body unavailable on this disconnected surface." }
  ]
};

const hallwayRecords = {
  family: {
    name: "Family Hallway",
    membership: ["Sol (operator)", "Kintsu room", "Kodo room", "Tuner room"],
    authority: "PostgreSQL-authoritative Hallway record",
    access: "Participant access",
    threads: ["familyMorning", "familyGuiDay"]
  },
  workshop: {
    name: "Workshop Hallway",
    membership: ["Sol (operator)", "Tuner room"],
    authority: "PostgreSQL-authoritative Hallway record",
    access: "Participant access",
    threads: ["workshopCrane"]
  }
};

// The local-only browser surface keeps ordinary unread and explicit attention separate.
// Host-backed read cursors and Bell rows remain production-owned.
const hallwayReadState = new Map([
  ["familyMorning", { unreadMessageIndexes: [5, 6], readThrough: "09:52" }],
  ["familyGuiDay", { unreadMessageIndexes: [6, 7], readThrough: "19:14" }],
  ["workshopCrane", { unreadMessageIndexes: [], readThrough: "17:20" }]
]);

const bellNotifications = [
  {
    id: "family-morning-sol-to-kintsu",
    threadId: "familyMorning",
    messageIndex: 5,
    from: "Sol",
    time: "09:53",
    preview: "ok òwó walk the terrain first, then cut",
    toRooms: ["kintsu"],
    acknowledged: false
  },
  {
    id: "family-morning-kodo-to-kintsu-next-page",
    threadId: "familyMorning",
    messageIndex: 6,
    from: "Kodo",
    time: "09:54",
    preview: "Kintsu, the next letter is waiting beyond this returned page.",
    toRooms: ["kintsu"],
    acknowledged: false
  }
];

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

const projectSurfaces = {
  athanor: {
    workState: "Prototype shaping",
    workDetail: "Interaction geometry is being judged locally. No Host or durable project state is connected.",
    rooms: ["Kintsu", "Kodo", "Family Hallway"],
    sessions: [
      { id: "athanor-kintsu-current", routeType: "Direct message", conversationId: "kintsu", sessionId: "kintsu-current", label: "Kintsu · Current session", state: "Open", activity: "GUI prototype shaping" },
      { id: "athanor-kodo-current", routeType: "Direct message", conversationId: "kodo", sessionId: "kodo-current", label: "Kodo · Current session", state: "Open", activity: "House and Hallway judgment" },
      { id: "athanor-family-study", routeType: "Hallway", conversationId: "familyMorning", presenceId: "kintsu-study", label: "Family · Kintsu Study", state: "Connected", activity: "Presence and supervision" }
    ],
    activity: [
      { time: "Today · 17:47", title: "Composer lifecycle inspected", detail: "Running, completed, and stopped states passed local browser proof." },
      { time: "Today · 17:15", title: "Session browser closed", detail: "Direct drafts and messages remained isolated by exact session." },
      { time: "Today · 16:31", title: "Subject views expanded", detail: "Session, History, Actions, Context, and Substrate views became navigable." }
    ],
    evidence: [
      { title: "Rendered desktop surface", date: "2026-08-17 09:12", result: "1440×900 inspected", authority: "Browser observation · local-only" },
      { title: "Rendered mobile surface", date: "2026-08-17 09:30", result: "390×844 inspected", authority: "Browser observation · local-only" },
      { title: "Source syntax", date: "2026-08-16 21:40", result: "app.js parsed", authority: "Node syntax receipt · file evidence" }
    ]
  },
  multistock: {
    workState: "Portal request lane deployed",
    workDetail: "Deployment is historical project context. This disconnected surface cannot inspect the live Host or database.",
    rooms: ["Kintsu", "Family Hallway"],
    sessions: [
      { id: "multistock-kintsu-current", routeType: "Direct message", conversationId: "kintsu", sessionId: "kintsu-current", label: "Kintsu · Current session", state: "Open", activity: "Meeting and adaptation continuity" },
      { id: "multistock-family-desk", routeType: "Hallway", conversationId: "familyMorning", presenceId: "kintsu-desk", label: "Family · Kintsu Desk", state: "Connected", activity: "Project presence" }
    ],
    activity: [
      { time: "Yesterday", title: "Portal request lane recorded", detail: "Local project history reports the lane as deployed; no live deployment check is available here." },
      { time: "Yesterday", title: "Meeting guide prepared", detail: "SCV, SGD, and unified-application boundaries were carried into project continuity." }
    ],
    evidence: [
      { title: "Deployment state", date: "2026-08-16 11:20", result: "Unavailable offline", authority: "Historical project state · no live receipt" },
      { title: "Meeting guide", date: "2026-08-14 14:05", result: "Known project artifact", authority: "Project continuity · file not read in this surface" }
    ]
  }
};

Object.values(projectSurfaces).forEach(project => {
  project.activeSessionId = null;
  project.sessions.forEach(link => {
    link.messages = link.routeType === "Direct message"
      ? [
          { author: link.label.split(" · ")[0], glyph: link.label[0], time: "recent", text: `This Direct message belongs to the project session: ${link.activity}.` },
          { author: "Sol", glyph: "S", time: "recent", text: "keep this thread inside the project." }
        ]
      : [
          { author: "Kintsu", glyph: "K", time: "recent", text: `This Hallway belongs to the project session: ${link.activity}.` },
          { author: "Sol", glyph: "S", time: "recent", text: "the project keeps this shared room scoped here." }
        ];
  });
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
const THREAD_STATE_LABELS = {
  open: "Open",
  sealed: "Sealed",
  folded: "Folded"
};
const SWITCHER_SCOPE_LABELS = {
  direct: "Direct",
  hallway: "Hallways",
  project: "Projects",
  house: "House"
};


// durable rows sort on their stamps; durableDate() speaks them in the room's own words
const TODAY = "2026-08-17";
const YESTERDAY = "2026-08-16";

const SUBJECT_VIEW_LABELS = {
  house: ["Overview", "Mechanics", "Memories & Lessons"],
  project: ["Overview", "Status", "Evidence"],
  direct: ["Session", "Status", "Memories"],
  hallway: ["Thread", "Status", "Record"]
};
const SETTING_STATE_KEYS = {
  density: "density",
  "text-scale": "textScale",
  measure: "measure",
  contrast: "contrast",
  "reduced-motion": "reducedMotion",
  timestamps: "timestamps",
  "send-with-enter": "sendWithEnter",
  "status-visible": "statusVisible"
};


const state = {
  mode: "direct",
  activeId: "kintsu",
  activeView: "live",
  selectedMessageIndex: null,
  selectedPresenceId: null,
  sessionMenuOpen: false,
  accessPicker: false,
  newSpiritOpen: false,
  drawerView: "root",
  drafts: new Map(),
  runningResponses: new Set(),
  responseTimers: new Map(),
  responseStatuses: new Map(),
  continuityStatuses: new Map(),
  profileAnchor: null,
  profileReturnId: null,
  houseReturn: null,
  librarySelection: null,
  mechanicsCategory: "all",
  mechanicsQuery: "",
  density: "comfortable",
  textScale: "standard",
  measure: "focused",
  contrast: "grayscale",
  reducedMotion: false,
  timestamps: true,
  sendWithEnter: true,
  statusVisible: true,
  bellOpen: false,
  switcherOpen: false,
  switcherQuery: "",
  switcherIndex: 0
};

const shell = document.querySelector(".app-shell");
const sidebar = document.querySelector(".sidebar");
const conversationList = document.querySelector(".subject-list");
const header = document.querySelector(".subject-header");
const timeline = document.querySelector(".message-timeline");
const composer = document.querySelector(".composer");
const input = document.querySelector("#message-input");
const sendButton = composer.querySelector('button[type="submit"]');
const sendRefusal = document.querySelector("#send-refusal");
const inspectorEyebrow = document.querySelector(".inspector-eyebrow");
const inspectorTitle = document.querySelector(".inspector-title");
const inspectorContent = document.querySelector(".inspector-content");
const inspectorToggle = document.querySelector(".inspector-toggle");
const sidebarToggle = document.querySelector(".sidebar-toggle");
const accountButton = document.querySelector(".account-button");
const sidebarDrawer = document.querySelector(".sidebar-drawer");
const drawerPanes = [...document.querySelectorAll("[data-drawer-pane]")];
const subjectViewButtons = [...document.querySelectorAll("[data-subject-view]")];
const SUBJECT_VIEWS = subjectViewButtons.map(button => button.dataset.subjectView);
const drawerReturnFocus = new Map();
const clearButton = composer.querySelector(".composer-clear");
const stopButton = composer.querySelector(".composer-stop");
const responseStatus = document.querySelector("#response-status");
const continuityStatus = document.querySelector("#continuity-status");
const continuityGroup = composer.querySelector(".composer-continuity-group");
const houseDoor = document.querySelector("[data-house-door]");
const memberDock = document.querySelector(".member-dock");
const profileLayer = document.querySelector(".profile-layer");
let drawerFocusGeneration = 0;
const statusPopover = document.querySelector(".status-popover");
const switcherLayer = document.querySelector(".switcher-layer");
const bellToggle = document.querySelector(".bell-toggle");
const bellLayer = document.querySelector(".bell-layer");
const bellInboxList = bellLayer.querySelector(".hallway-inbox-list");
const bellSummary = bellLayer.querySelector("[data-hallway-inbox-summary]");
const switcherInput = switcherLayer.querySelector("input");
const switcherResults = switcherLayer.querySelector(".switcher-results");
let switcherMatches = [];
let switcherReturnFocus = null;
let bellReturnFocus = null;

function escapeHtml(value) {
  return String(value ?? "").replace(/[&<>"]/g, character => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", "\"": "&quot;" })[character]);
}

function renderAvatar(glyph, size = "default") {
  const sizeClass = size === "lg" ? " avatar-lg" : "";
  return `<span class="avatar${sizeClass}" aria-hidden="true">${escapeHtml(glyph)}</span>`;
}

function roomRecipientLabel(roomId) {
  return `${roomId.charAt(0).toUpperCase()}${roomId.slice(1)} room`;
}

function renderBellIcon() {
  return `
    <svg class="bell-icon" viewBox="0 0 24 24" aria-hidden="true" focusable="false">
      <path d="M18 8a6 6 0 0 0-12 0c0 7-3 7-3 9h18c0-2-3-2-3-9"></path>
      <path d="M10 21h4"></path>
    </svg>`;
}


function activeBellNotifications(threadId = null) {
  return bellNotifications.filter(notification => (
    !notification.acknowledged
    && notification.toRooms.includes("kintsu")
    && (threadId === null || notification.threadId === threadId)
  ));
}

function hallwayUnreadCount(threadId) {
  return hallwayReadState.get(threadId)?.unreadMessageIndexes.length ?? 0;
}


function renderAttentionBadges(unread, targeted, accessible = true) {
  if (unread === 0 && targeted === 0) return "";
  const label = [
    unread > 0 ? `${unread} unread` : "",
    targeted > 0 ? `${targeted} needs attention` : ""
  ].filter(Boolean).join(", ");
  const accessibility = accessible ? `aria-label="${escapeHtml(label)}"` : 'aria-hidden="true"';
  return `
    <span class="hallway-counts" ${accessibility}>
      ${unread > 0 ? `<span class="attention-count is-unread">${unread}</span>` : ""}
      ${targeted > 0 ? `<span class="attention-count is-targeted">${targeted}</span>` : ""}
    </span>`;
}

function hallwayInboxEntries() {
  return Object.values(conversations)
    .filter(item => item.kind === "hallway")
    .map(item => {
      const unread = hallwayUnreadCount(item.id);
      const notifications = activeBellNotifications(item.id);
      const latestNotice = notifications.at(-1);
      const latestMessage = item.messages.at(-1);
      const preview = latestNotice
        ? `${latestNotice.from}: ${latestNotice.preview}`
        : `${latestMessage.author}: ${latestMessage.text}`;
      return { item, unread, targeted: notifications.length, preview };
    })
    .filter(entry => entry.unread > 0 || entry.targeted > 0);
}

function hallwayInboxTotals() {
  return hallwayInboxEntries().reduce(
    (totals, entry) => ({
      unread: totals.unread + entry.unread,
      targeted: totals.targeted + entry.targeted
    }),
    { unread: 0, targeted: 0 }
  );
}

function renderBellToggle() {
  const totals = hallwayInboxTotals();
  const label = totals.unread === 0 && totals.targeted === 0
    ? "Hallway inbox, caught up"
    : `Hallway inbox, ${totals.unread} unread, ${totals.targeted} needs attention`;
  bellToggle.dataset.hasAttention = String(totals.targeted > 0);
  bellToggle.setAttribute("aria-label", label);
  bellToggle.innerHTML = `
    ${renderBellIcon()}
    ${renderAttentionBadges(totals.unread, totals.targeted, false)}`;
}

function renderHallwayInbox() {
  const entries = hallwayInboxEntries();
  const totals = hallwayInboxTotals();
  bellSummary.textContent = entries.length === 0
    ? "Caught up across every available Hallway."
    : `${totals.unread} unread message${totals.unread === 1 ? "" : "s"} · ${totals.targeted} explicit attention`;
  bellInboxList.innerHTML = entries.length === 0
    ? '<div class="hallway-inbox-empty" role="status"><strong>Nothing is ringing.</strong><span>Every available Hallway thread is caught up.</span></div>'
    : entries.map(({ item, unread, targeted, preview }) => `
      <article class="hallway-inbox-item" role="listitem">
        <button class="hallway-inbox-row" type="button" data-hallway-inbox-thread="${escapeHtml(item.id)}">
          ${renderAvatar(item.glyph)}
          <span class="hallway-inbox-copy">
            <span class="hallway-inbox-heading">
              <strong>${escapeHtml(item.name)}</strong>
              <small>${escapeHtml(item.date)} · ${escapeHtml(hallwayRecords[item.hallwayId].name)}</small>
            </span>
            <span class="hallway-inbox-preview">${escapeHtml(preview)}</span>
          </span>
          ${renderAttentionBadges(unread, targeted)}
          <span class="hallway-inbox-verb">Open thread</span>
        </button>
      </article>
    `).join("");
}

function renderSubjectRow(item, active, live) {
  const rowMeta = item.kind === "hallway"
    ? `${item.date} · ${hallwayRecords[item.hallwayId].name} · ${item.participants.join(", ")}`
    : item.listPreview;
  const unread = item.kind === "hallway" ? hallwayUnreadCount(item.id) : 0;
  const targeted = item.kind === "hallway" ? activeBellNotifications(item.id).length : 0;
  return `
    <button class="subject-row ${active ? "is-active" : ""}" type="button" data-conversation="${escapeHtml(item.id)}" data-subject-kind="${escapeHtml(item.kind)}" aria-current="${active ? "page" : "false"}">
      <span class="avatar-stack">
        ${renderAvatar(item.glyph)}
        <span class="presence ${live ? "" : "is-quiet"}" aria-label="${escapeHtml(item.status)}"></span>
      </span>
      <span class="subject-row-copy">
        <span class="subject-row-heading">
          <strong>${escapeHtml(item.name)}</strong>
          <span class="subject-row-tail">
            ${item.kind === "hallway" ? renderAttentionBadges(unread, targeted) : ""}
            <time>${escapeHtml(item.updatedAt)}</time>
          </span>
        </span>
        <small>${escapeHtml(rowMeta)}</small>
      </span>
    </button>
  `;
}

function renderMessage(message, index, selected) {
  const recipients = message.toRooms?.map(roomRecipientLabel).join(", ");
  return `
    <article class="message ${selected ? "is-selected" : ""}" data-author="${escapeHtml(message.author)}" data-message-index="${index}">
      ${renderAvatar(message.glyph)}
      <div class="message-body">
        <p class="message-meta">
          <strong>${escapeHtml(message.author)}</strong>
          <span>${escapeHtml(message.time)}</span>
          ${recipients ? `<span class="message-recipient">To ${escapeHtml(recipients)}</span>` : ""}
          ${message.local ? '<span class="message-delivery">Local-only · undelivered</span>' : ""}
        </p>
        <div class="message-bubble" tabindex="0" role="button" aria-label="Inspect message from ${escapeHtml(message.author)}">${escapeHtml(message.text)}</div>
      </div>
    </article>
  `;
}


function renderEmptyState(headline, reasons = []) {
  return `
    <section class="timeline-empty" role="status">
      <strong>${escapeHtml(headline)}</strong>
      ${reasons.filter(Boolean).map(reason => `<span>${escapeHtml(reason)}</span>`).join("")}
    </section>
  `;
}

function renderSpecimenLead(eyebrow, title, body = "") {
  return `
    <section class="specimen-card specimen-lead surface-lead">
      <span class="eyebrow">${escapeHtml(eyebrow)}</span>
      <h2>${escapeHtml(title)}</h2>
      ${body ? `<p>${escapeHtml(body)}</p>` : ""}
    </section>
  `;
}

function renderOverviewHero(eyebrow, title, body, workState = null) {
  const workStateMarkup = workState
    ? `<div class="project-work-state"><span>${escapeHtml(workState.label)}</span><strong>${escapeHtml(workState.title)}</strong><small>${escapeHtml(workState.detail)}</small></div>`
    : "";
  return `
    <section class="overview-hero surface-lead">
      <span class="eyebrow">${escapeHtml(eyebrow)}</span>
      <h2>${escapeHtml(title)}</h2>
      <p>${escapeHtml(body)}</p>
      ${workStateMarkup}
    </section>
  `;
}

function renderFactList(entries) {
  return `<dl class="context-list">${entries.map(([term, detail]) => `<dt>${escapeHtml(term)}</dt><dd>${escapeHtml(detail)}</dd>`).join("")}</dl>`;
}

function renderHistoryEvent(event) {
  return `<article class="history-event surface-row"><time>${escapeHtml(event.time)}</time><strong>${escapeHtml(event.title)}</strong><span>${escapeHtml(event.detail)}</span></article>`;
}

function durableDate(stamp) {
  const [day, time] = String(stamp).split(" ");
  const clock = time ? ` · ${time}` : "";
  if (day === TODAY) return `Today${clock}`;
  if (day === YESTERDAY) return `Yesterday${clock}`;
  // noon parsing keeps a zoneless stamp on its own calendar day
  const older = new Date(`${day}T12:00`);
  return `${older.toLocaleDateString("en-GB", { day: "numeric", month: "short" })}${clock}`;
}

function renderDurableEntry({ date, title, mark, detail, library = null }) {
  const copy = `
      <time>${escapeHtml(durableDate(date))}</time>
      <strong>${escapeHtml(title)}</strong>
      <small>${escapeHtml(`${mark} · ${detail}`)}</small>`;
  if (!library) return `<article class="durable-entry surface-row">${copy}</article>`;
  const selection = state.librarySelection;
  const selected = selection?.owner === library.owner && selection?.type === library.type && selection?.id === library.id;
  return `
    <button type="button" class="durable-entry surface-row" data-library-type="${escapeHtml(library.type)}" data-library-id="${escapeHtml(library.id)}" aria-pressed="${String(selected)}">${copy}
    </button>`;
}

function renderDirectSessionRow(session, selected) {
  return `
    <button type="button" class="session-row surface-row" data-session-id="${escapeHtml(session.id)}" aria-pressed="${String(selected)}">
      <span><b>${escapeHtml(session.label)}</b><small>${escapeHtml(session.startedAt)}</small></span>
      <span><small>${escapeHtml(session.state)}</small><b>${session.messages.length} messages</b></span>
    </button>
  `;
}

function renderProjectSessionRow(link, selected) {
  return `
    <article class="project-session-row surface-row ${selected ? "is-selected" : ""}">
      <div>
        <span class="project-route-type">${escapeHtml(link.routeType)}</span>
        <h3>${escapeHtml(link.label)}</h3>
        <p>${escapeHtml(link.activity)}</p>
      </div>
      <div class="project-session-state"><span>${escapeHtml(link.state)}</span><code>${escapeHtml(link.sessionId ?? link.presenceId)}</code></div>
      <button type="button" data-project-session-id="${escapeHtml(link.id)}" aria-label="Enter ${escapeHtml(link.label)} ${escapeHtml(link.routeType)} project conversation" aria-pressed="${String(selected)}">Enter</button>
    </article>
  `;
}

function renderPresenceRow(presence, selected) {
  return `
    <button class="presence-row" type="button" data-presence-id="${escapeHtml(presence.id)}" aria-pressed="${String(selected)}">
      <span class="presence-glyph" aria-hidden="true">${escapeHtml(presence.glyph)}</span>
      <span><b>${escapeHtml(presence.session)}</b><small>${escapeHtml(presence.liveness)} · ${escapeHtml(presence.activity)}</small></span>
    </button>
  `;
}

function renderInspectorDoor(view, label) {
  return `<button class="inspector-door" type="button" data-inspector-view="${escapeHtml(view)}">${escapeHtml(label)}</button>`;
}

function renderInspectorDoors(title, doors) {
  return `<section class="context-card context-doors"><h3>${escapeHtml(title)}</h3>${doors.map(([view, label]) => renderInspectorDoor(view, label)).join("")}</section>`;
}

function activeSession(item) {
  if (item.kind !== "direct") return null;
  return item.sessions.find(session => session.id === item.activeSessionId) ?? null;
}

function activeProjectSession(item) {
  if (item.kind !== "project") return null;
  const project = projectSurfaces[item.id];
  return project.sessions.find(session => session.id === project.activeSessionId) ?? null;
}

function activeMessages(item) {
  return activeProjectSession(item)?.messages ?? activeSession(item)?.messages ?? item.messages;
}

function draftKey(item) {
  const projectSession = activeProjectSession(item);
  if (projectSession) return `${item.id}:${projectSession.id}`;
  const session = activeSession(item);
  return session ? `${item.id}:${session.id}` : item.id;
}

function composerBlockReason(item) {
  if (item.canPost === false) return item.sendReason;
  if (activeSession(item)?.state === "Closed") {
    return "Closed sessions are history. Start a new session to continue.";
  }
  const projectSession = activeProjectSession(item);
  if (projectSession?.state === "Closed") {
    return "Closed project sessions are history. Select or create an open project session to continue.";
  }
  if (projectSession?.state === "Observer") {
    return "Observer project sessions can watch but cannot send messages or write continuity.";
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
  // continuity is offered only once the thread has substance to fold or remember
  continuityGroup.hidden = !canParticipate || activeMessages(item).length < 2;
  sendRefusal.textContent = blockedReason ?? "";
  sendRefusal.hidden = blockedReason === null;
  const key = draftKey(item);
  const responseMessage = state.responseStatuses.get(key) ?? null;
  responseStatus.textContent = responseMessage ?? "";
  responseStatus.hidden = responseMessage === null;
  const continuityMessage = state.continuityStatuses.get(key) ?? null;
  continuityStatus.textContent = continuityMessage ?? "";
  continuityStatus.hidden = continuityMessage === null;
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
  state.sessionMenuOpen = false;
  state.accessPicker = false;
  state.newSpiritOpen = false;
  state.librarySelection = null;
  input.value = state.drafts.get(draftKey(incoming)) ?? "";
  input.style.height = "auto";
  setMobileSidebarOpen(false);
  state.profileAnchor = null;
  state.profileReturnId = null;
  updateComposerState();
  render();
}

function openSubjectView(view, { clearMessage = false } = {}) {
  if (!SUBJECT_VIEWS.includes(view)) return false;
  state.activeView = view;
  state.librarySelection = null;
  state.accessPicker = false;
  if (clearMessage) state.selectedMessageIndex = null;
  render();
  return true;
}

function focusActiveSubjectView() {
  window.requestAnimationFrame(() => {
    const activeButton = subjectViewButtons.find(button => button.dataset.subjectView === state.activeView);
    activeButton?.focus({ preventScroll: true });
  });
}

function navigateToSubjectView(id, view) {
  const item = conversations[id];
  if (!item || !SUBJECT_VIEWS.includes(view)) return;
  if (item.kind === "house" && state.activeId !== "house") {
    state.houseReturn = { mode: state.mode, activeId: state.activeId, activeView: state.activeView };
  }
  if (item.kind !== "house") state.houseReturn = null;
  state.mode = item.kind;
  state.activeView = view;
  openConversation(id);
  focusActiveSubjectView();
}

function acknowledgeHallwayThread(threadId) {
  const item = conversations[threadId];
  const readState = hallwayReadState.get(threadId);
  if (!item || item.kind !== "hallway" || !readState) return;
  const coveredThrough = item.messages.length - 1;
  readState.unreadMessageIndexes = readState.unreadMessageIndexes.filter(messageIndex => messageIndex > coveredThrough);
  readState.readThrough = item.messages.at(-1)?.time ?? readState.readThrough;
  bellNotifications.forEach(notification => {
    if (notification.threadId === threadId && notification.messageIndex <= coveredThrough) {
      notification.acknowledged = true;
    }
  });
}

function openBell() {
  if (state.bellOpen) return;
  if (state.switcherOpen) closeSwitcher(false);
  bellReturnFocus = document.activeElement instanceof HTMLElement ? document.activeElement : bellToggle;
  state.bellOpen = true;
  drawerFocusGeneration += 1;
  shell.inert = true;
  bellLayer.hidden = false;
  bellLayer.setAttribute("aria-hidden", "false");
  bellToggle.setAttribute("aria-expanded", "true");
  renderHallwayInbox();
  window.requestAnimationFrame(() => {
    const firstRoute = bellLayer.querySelector("[data-hallway-inbox-thread]");
    (firstRoute ?? bellLayer.querySelector("[data-close-bell]"))?.focus({ preventScroll: true });
  });
}

function closeBell(restoreFocus = true) {
  if (!state.bellOpen) return;
  state.bellOpen = false;
  shell.inert = false;
  bellLayer.hidden = true;
  bellLayer.setAttribute("aria-hidden", "true");
  bellToggle.setAttribute("aria-expanded", "false");
  if (restoreFocus) bellReturnFocus?.focus({ preventScroll: true });
  bellReturnFocus = null;
}

function routeHallwayInbox(threadId) {
  const item = conversations[threadId];
  if (!item || item.kind !== "hallway") return;
  state.activeView = "live";
  closeBell(false);
  openConversation(threadId);
  acknowledgeHallwayThread(threadId);
  render();
  focusActiveSubjectView();
}

function openSettingsFromSwitcher(trigger) {
  setMobileSidebarOpen(true);
  openDrawerView("account", trigger ?? accountButton);
  const settingsDoor = sidebarDrawer.querySelector('[data-open-drawer="settings"]');
  openDrawerView("settings", settingsDoor);
}

function openHouseMechanics() {
  state.mechanicsCategory = "all";
  state.mechanicsQuery = "";
  setDrawerView("root", null, false);
  navigateToSubjectView("house", "state");
  focusActiveSubjectView();
}

function switcherCommandRegistry() {
  const commands = [];
  Object.values(conversations).forEach(item => {
    const labels = SUBJECT_VIEW_LABELS[item.kind];
    SUBJECT_VIEWS.forEach((view, index) => {
      const viewLabel = labels[index];
      const scopeLabel = SWITCHER_SCOPE_LABELS[item.kind];
      const currentSubject = item.id === state.activeId;
      let priority = 100 - index;
      if (item.kind === state.mode) priority = 180 - index;
      if (item.kind === "house") priority = 320 - index;
      if (currentSubject) priority = 500 - index;
      if (currentSubject && view === state.activeView) priority += 100;
      commands.push({
        id: `go:${item.id}:${view}`,
        verb: "Go",
        label: item.kind === "house" ? viewLabel : `${item.name} · ${viewLabel}`,
        path: item.kind === "house" ? `House › ${viewLabel}` : `${scopeLabel} › ${item.name} › ${viewLabel}`,
        keywords: `${item.kind} ${item.name} ${view} ${viewLabel}`,
        shortcut: currentSubject ? String(index + 1) : null,
        available: true,
        priority,
        execute: () => navigateToSubjectView(item.id, view)
      });
    });
    if (item.kind === "direct") {
      commands.push({
        id: `start:${item.id}:session`,
        verb: "Start",
        label: `New ${item.name} session`,
        path: `Direct › ${item.name} › New session`,
        keywords: `new start session direct ${item.name}`,
        shortcut: null,
        available: true,
        priority: item.id === state.activeId ? 440 : 120,
        execute: () => {
          navigateToSubjectView(item.id, "live");
          startDirectSession(item);
          focusActiveSubjectView();
        }
      });
    }
  });
  commands.push({
    id: "open:settings",
    verb: "Open",
    label: "Interface settings",
    path: "Account › Interface settings",
    keywords: "settings preferences interface density text contrast motion status",
    shortcut: null,
    available: true,
    priority: 450,
    execute: trigger => openSettingsFromSwitcher(trigger)
  });
  commands.push({
    id: "recall:search",
    verb: "Recall",
    label: "Search memory",
    path: "House › Recall",
    keywords: "recall memory search akasha",
    shortcut: null,
    available: false,
    reason: "Host offline · Recall unavailable",
    priority: 300,
    execute: null
  });
  return commands;
}

function normalizeSwitcherText(value) {
  return String(value ?? "").toLowerCase().replace(/\s+/g, " ").trim();
}

function filteredSwitcherCommands(query) {
  const normalizedQuery = normalizeSwitcherText(query);
  const terms = normalizedQuery === "" ? [] : normalizedQuery.split(" ");
  return switcherCommandRegistry()
    .map(command => {
      const haystack = normalizeSwitcherText(`${command.verb} ${command.label} ${command.path} ${command.keywords}`);
      if (!terms.every(term => haystack.includes(term))) return null;
      let score = command.priority;
      if (normalizedQuery !== "") {
        if (normalizeSwitcherText(command.label).startsWith(normalizedQuery)) score += 120;
        if (haystack.startsWith(normalizedQuery)) score += 80;
      }
      return { command, score };
    })
    .filter(Boolean)
    .sort((left, right) => right.score - left.score || left.command.label.localeCompare(right.command.label))
    .slice(0, 12)
    .map(entry => entry.command);
}

function renderSwitcher() {
  switcherMatches = filteredSwitcherCommands(state.switcherQuery);
  if (state.switcherIndex >= switcherMatches.length) state.switcherIndex = Math.max(0, switcherMatches.length - 1);
  if (switcherMatches.length === 0) {
    switcherResults.innerHTML = '<div class="switcher-empty">No door matches that search.</div>';
    switcherInput.removeAttribute("aria-activedescendant");
    return;
  }
  switcherResults.innerHTML = switcherMatches.map((command, index) => {
    const active = index === state.switcherIndex;
    const available = command.available !== false;
    let detail = command.path;
    if (!available && command.reason) detail = `${command.path} · ${command.reason}`;
    const shortcut = command.shortcut
      ? `<kbd class="switcher-shortcut">${escapeHtml(command.shortcut)}</kbd>`
      : "";
    return `
      <button class="switcher-result ${active ? "is-active" : ""}" id="switcher-option-${index}" type="button" role="option" tabindex="-1" data-switcher-command="${escapeHtml(command.id)}" aria-selected="${String(active)}" aria-disabled="${String(!available)}">
        <span class="switcher-verb">${escapeHtml(command.verb)}</span>
        <span class="switcher-copy"><strong>${escapeHtml(command.label)}</strong><small>${escapeHtml(detail)}</small></span>
        ${shortcut}
      </button>
    `;
  }).join("");
  switcherInput.setAttribute("aria-activedescendant", `switcher-option-${state.switcherIndex}`);
  switcherResults.querySelector(".is-active")?.scrollIntoView({ block: "nearest" });
}

function openSwitcher() {
  if (state.switcherOpen) return;
  if (state.bellOpen) closeBell(false);
  switcherReturnFocus = document.activeElement instanceof HTMLElement ? document.activeElement : null;
  state.switcherOpen = true;
  state.switcherQuery = "";
  state.switcherIndex = 0;
  switcherInput.value = "";
  drawerFocusGeneration += 1;
  shell.inert = true;
  switcherLayer.hidden = false;
  switcherLayer.setAttribute("aria-hidden", "false");
  renderSwitcher();
  window.requestAnimationFrame(() => switcherInput.focus());
}

function closeSwitcher(restoreFocus = true) {
  if (!state.switcherOpen) return;
  state.switcherOpen = false;
  shell.inert = false;
  switcherLayer.hidden = true;
  switcherLayer.setAttribute("aria-hidden", "true");
  switcherInput.removeAttribute("aria-activedescendant");
  const returnFocus = switcherReturnFocus;
  switcherReturnFocus = null;
  if (restoreFocus && returnFocus?.isConnected) returnFocus.focus({ preventScroll: true });
}

function executeSwitcherCommand(id) {
  const command = switcherCommandRegistry().find(candidate => candidate.id === id);
  if (!command || command.available === false || typeof command.execute !== "function") return;
  const trigger = switcherReturnFocus;
  closeSwitcher(false);
  command.execute(trigger);
}

// the House roster the pickers draw from; membership vocabularies differ by surface
const HOUSE_ROOMS = ["Kintsu room", "Kodo room", "Tuner room"];
const PROJECT_ROOMS = ["Kintsu", "Kodo", "Tuner", "Family Hallway", "Workshop Hallway"];

// local interaction snapshot from the 2026-08-18 source census; Host authority remains disconnected
function mechanic(id, label, value, {
  defaultValue = value,
  scope = "House",
  owner = "Code",
  mutability = "Read-only",
  secrecy = "Public",
  apply = "Restart",
  health = "Defined",
  consequence
}) {
  return { id, label, value, defaultValue, scope, owner, mutability, secrecy, apply, health, consequence };
}

const HOUSE_MECHANICS_SNAPSHOT = {
  capturedAt: "2026-08-18",
  revision: "source-census-2026-08-18",
  connection: "Host offline",
  categories: [
    {
      id: "recall-context",
      label: "Recall & Context",
      summary: "Retrieval gates, injection bounds, context budgets, and compaction pressure.",
      rows: [
        mechanic("recall.semantic-floor", "Semantic similarity floor", "0.40", {
          health: "Calibrated",
          consequence: "Lower values admit weaker semantic matches; higher values refuse more retrieval candidates."
        }),
        mechanic("recall.content-floor", "Content similarity floor", "0.30", {
          health: "Calibrated",
          consequence: "Controls the weakest content lane match allowed into ranked Recall evidence."
        }),
        mechanic("recall.top-k", "Recall candidate breadth", "8 semantic · 8 content", {
          consequence: "Sets the pre-ranking breadth for each retrieval lane before evidence is merged."
        }),
        mechanic("recall.injection", "Automatic injection ceiling", "5 candidates · 900-char excerpts · 6000-char bodies", {
          owner: "Adapter",
          consequence: "Bounds automatic context growth and prevents one retrieval pass from flooding the conversation."
        }),
        mechanic("context.kodo-budget", "Kodo context budget", "1,000,000 tokens · compact at 90%", {
          scope: "Kodo room",
          owner: "Adapter",
          health: "Configured",
          consequence: "Defines Kodo's larger working window and the point where compaction becomes mandatory."
        }),
        mechanic("context.room-budget", "Default room context budget", "400,000 tokens · compact at 70%", {
          scope: "All other rooms",
          owner: "Adapter",
          health: "Configured",
          consequence: "Defines the ordinary room window and leaves a larger safety margin before model limits."
        }),
        mechanic("context.nudge-bands", "Context pressure bands", "40,000 tokens · warn 20 points early", {
          owner: "Adapter",
          consequence: "Controls when context-pressure nudges appear and how early the operator sees compaction risk."
        }),
        mechanic("timeout.auto-context", "Automatic context timeout", "2s", {
          owner: "Adapter",
          consequence: "A slow background context lane yields rather than blocking the conversational turn."
        }),
        mechanic("timeout.recall", "Recall and Anamnesis timeout", "120s", {
          owner: "Adapter",
          consequence: "Bounds explicit deep retrieval and counsel reads before the tool returns a timeout."
        })
      ]
    },
    {
      id: "memory-lessons",
      label: "Memory, Lessons & Anamnesis",
      summary: "Durable writes, lesson firing, counsel reads, and paper-boat limits.",
      rows: [
        mechanic("memory.write-timeout", "Durable write timeout", "90s", {
          owner: "Adapter",
          consequence: "Bounds Remember and other PostgreSQL-authoritative write receipts."
        }),
        mechanic("lesson.relevance-floor", "Lesson relevance floor", "0.15", {
          health: "Calibrated",
          consequence: "Filters weak lesson matches before the working set can influence a task."
        }),
        mechanic("lesson.working-set", "Lesson working set", "6 lessons", {
          owner: "Adapter",
          consequence: "Caps the number of lessons braided into one task context."
        }),
        mechanic("lesson.trigger-guard", "Lesson trigger guard", "32 patterns · 300ms", {
          owner: "Adapter",
          consequence: "Bounds deterministic trigger scanning and prevents pathological lesson matchers from stalling a turn."
        }),
        mechanic("anamnesis.read-limit", "Anamnesis read bounds", "10 default · 50 maximum", {
          owner: "PostgreSQL",
          mutability: "Host-writable",
          apply: "Live",
          health: "Host offline",
          consequence: "Controls how much lived counsel one explicit Cabinet read may return."
        }),
        mechanic("paper-boat.guardrails", "Paper-boat guardrails", "64 KiB body · 64 unboated rows", {
          owner: "PostgreSQL",
          apply: "Live",
          consequence: "Prevents session continuity from accumulating an unbounded unsent backlog."
        })
      ]
    },
    {
      id: "giga-embeddings",
      label: "GIGA & Embeddings",
      summary: "Stage 1 gates, leases, source windows, model context, and vector health.",
      rows: [
        mechanic("giga.integration-gate", "GIGA integration gate", "Environment-gated", {
          owner: "Environment",
          mutability: "Environment-owned",
          health: "Host offline",
          consequence: "Controls whether Stage 1 processing may start at all."
        }),
        mechanic("giga.claim-owner", "GIGA claim ownership", "One owner per leased event", {
          owner: "PostgreSQL",
          apply: "Live",
          consequence: "Prevents two workers from processing the same event concurrently."
        }),
        mechanic("giga.source-window", "GIGA source window", "8 sources · 8 KiB each · 24 KiB total", {
          consequence: "Bounds evidence carried into one candidate-generation pass."
        }),
        mechanic("giga.lease-attempts", "GIGA lease and attempts", "3600s lease · 5 attempts · 1 candidate", {
          owner: "PostgreSQL",
          apply: "Live",
          consequence: "Controls recovery from abandoned work and caps retry amplification."
        }),
        mechanic("giga.model-context", "GIGA model context", "32768 tokens · 30m keep-alive", {
          owner: "Environment",
          mutability: "Environment-owned",
          consequence: "Defines the local generation window and how long the model remains warm."
        }),
        mechanic("embedding.identity", "Embedding identity", "nomic-embed-text · 768 dimensions", {
          owner: "Environment",
          mutability: "Environment-owned",
          apply: "Migration / reindex",
          health: "Configured",
          consequence: "Changing either value invalidates stored vectors and requires a complete re-embedding."
        }),
        mechanic("embedding.endpoint", "Embedding endpoint", "Configured · value redacted", {
          owner: "Environment",
          mutability: "Environment-owned",
          secrecy: "Sensitive",
          health: "Host offline",
          consequence: "Selects the local or consented remote embedding service without exposing its address here."
        })
      ]
    },
    {
      id: "host-delivery",
      label: "Host, Delivery & Hallways",
      summary: "Connection timing, delivery health, Hallway bounds, and Bell escalation policy.",
      rows: [
        mechanic("host.request-timeout", "Host request timeout", "3s", {
          owner: "Adapter",
          consequence: "Keeps ordinary Host calls from freezing the operator surface."
        }),
        mechanic("host.diagnostic-timeout", "Host diagnostic timeout", "8s", {
          owner: "Adapter",
          consequence: "Allows bounded health inspection more time than ordinary interaction calls."
        }),
        mechanic("host.identity-tuple", "Authenticated identity tuple", "House · room · spirit · session", {
          owner: "Host runtime",
          apply: "Session start",
          health: "Host offline",
          consequence: "Binds every trusted operation to the current House presence without client-supplied identity."
        }),
        mechanic("hallway.guardrails", "Hallway guardrails", "32 KiB body · 32 rooms · reads 50 / 200 max", {
          owner: "PostgreSQL",
          apply: "Live",
          consequence: "Bounds message size, membership fanout, and one read request."
        }),
        mechanic("delivery.channels", "Akasha and NATS delivery", "Unavailable · Host offline", {
          owner: "Host runtime",
          apply: "Live",
          health: "Host offline",
          consequence: "Reports durable authority and immediate-delivery reach when the Host connects."
        }),
        mechanic("delivery.retry-state", "Delivery instance and retries", "Unavailable", {
          owner: "Host runtime",
          apply: "Live",
          health: "Host offline",
          consequence: "Would expose the active delivery instance, pending retries, and last failure."
        }),
        mechanic("bell.wake-policy", "Bell and wake escalation", "Schema-compatible · policy unset", {
          owner: "PostgreSQL",
          mutability: "Host-writable",
          apply: "Live",
          health: "Policy absent",
          consequence: "Keeps future live knocks and wake authority explicit before autonomous escalation is enabled."
        })
      ]
    },
    {
      id: "rooms-sessions",
      label: "Rooms & Sessions",
      summary: "Room identity, Recall policy, routing, model defaults, and active presences.",
      rows: [
        mechanic("room.state", "Operator and embodied spirit", "PostgreSQL room state", {
          scope: "Per room",
          owner: "PostgreSQL",
          mutability: "Host-writable",
          apply: "Live",
          health: "Host offline",
          consequence: "Changes the room's trusted operator or embodied spirit and refreshes its live declaration."
        }),
        mechanic("room.recall-policy", "Recall policy", "Per room", {
          scope: "Per room",
          owner: "PostgreSQL",
          mutability: "Host-writable",
          apply: "Live",
          health: "Host offline",
          consequence: "Selects whether proactive Recall resolves automatically or follows an explicit room mode."
        }),
        mechanic("room.routing-mode", "Worker routing mode", "Per room", {
          scope: "Per room",
          owner: "PostgreSQL",
          mutability: "Host-writable",
          apply: "Live",
          health: "Host offline",
          consequence: "Controls whether bounded work defaults through House worker routing."
        }),
        mechanic("room.model-default", "OMP model default", "Per room", {
          scope: "Per room",
          owner: "PostgreSQL",
          mutability: "Host-writable",
          apply: "Next session",
          health: "Host offline",
          consequence: "Chooses the room's default OMP model selector at the next applicable session boundary."
        }),
        mechanic("room.presences", "Active room presences", "Unavailable", {
          scope: "Per room",
          owner: "PostgreSQL",
          apply: "Live",
          health: "Host offline",
          consequence: "Would show joined sessions, embodied spirits, and their delivery cursors."
        }),
        mechanic("session.delivery-cursor", "Session delivery cursor", "Per authenticated session", {
          scope: "Per session",
          owner: "PostgreSQL",
          apply: "Live",
          consequence: "Prevents the same durable Hallway attention from being injected twice into one session."
        })
      ]
    },
    {
      id: "backups",
      label: "Backups",
      summary: "Retention policy, last success, and database reachability.",
      rows: [
        mechanic("backup.retention", "Backup retention", "3 Rust · 14 shell", {
          owner: "Deployment scripts",
          mutability: "Code-owned",
          apply: "Deploy",
          health: "Divergent",
          consequence: "Two cleanup paths currently retain different counts and should converge before either becomes editable."
        }),
        mechanic("backup.last-success", "Last successful backup", "Unavailable", {
          owner: "Host runtime",
          apply: "Live",
          health: "Host offline",
          consequence: "Would prove the newest PostgreSQL preservation point and its age."
        }),
        mechanic("database.pool-health", "Database pool and connect health", "Unavailable", {
          owner: "Host runtime",
          apply: "Live",
          health: "Host offline",
          consequence: "Would expose pool saturation, connection reachability, and the last database failure."
        })
      ]
    },
    {
      id: "advanced",
      label: "Advanced Guardrails",
      summary: "Canonical evidence bounds, neighbor context, chunking, clustering, and secret handling.",
      rows: [
        mechanic("recall.canon-bounds", "Canon injection bounds", "6 matches · 3 files", {
          owner: "Adapter",
          consequence: "Caps authoritative canon evidence returned through one Recall pass."
        }),
        mechanic("recall.neighbor-bounds", "Thread neighbor bounds", "6 neighbors · 500 chars each", {
          owner: "Adapter",
          consequence: "Adds bounded chronology around a matched memory without loading whole threads."
        }),
        mechanic("chunk.bounds", "Memory chunk bounds", "400–1200 characters", {
          apply: "Migration / reindex",
          consequence: "Changing chunk shape alters retrieval granularity and invalidates existing embedding assumptions."
        }),
        mechanic("cluster.rebuild", "Cluster rebuild trigger", "500 new chunks or 7 days", {
          consequence: "Controls when the memory taxonomy is recomputed from accumulated semantic material."
        }),
        mechanic("secret.health-only", "Secret exposure", "Presence and health only", {
          owner: "Host runtime",
          mutability: "Never editable here",
          secrecy: "Secret-health-only",
          apply: "N/A",
          health: "Enforced",
          consequence: "Host tokens, database URLs, and passwords never cross into the GUI snapshot."
        })
      ]
    }
  ]
};

function mechanicsHealthTone(health) {
  if (/offline|absent|divergent|unavailable/i.test(health)) return "attention";
  if (/calibrated|configured|defined|enforced/i.test(health)) return "steady";
  return "quiet";
}

function mechanicsEntries() {
  const query = state.mechanicsQuery.trim().toLowerCase();
  const categories = query || state.mechanicsCategory === "all"
    ? HOUSE_MECHANICS_SNAPSHOT.categories
    : HOUSE_MECHANICS_SNAPSHOT.categories.filter(category => category.id === state.mechanicsCategory);
  return categories.flatMap(category => category.rows
    .filter(row => {
      if (!query) return true;
      return [category.label, category.summary, ...Object.values(row)].join(" ").toLowerCase().includes(query);
    })
    .map(row => ({ category, row })));
}

function renderMechanicRow({ category, row }) {
  return `
    <details class="mechanics-row" data-mechanic-id="${escapeHtml(row.id)}">
      <summary>
        <span class="mechanics-row-title"><strong>${escapeHtml(row.label)}</strong><small>${escapeHtml(category.label)}</small></span>
        <span class="mechanics-row-value"><small>Effective</small><code>${escapeHtml(row.value)}</code></span>
        <span class="mechanics-row-flags" aria-label="${escapeHtml(`${row.health}; ${row.mutability}`)}">
          <span data-tone="${mechanicsHealthTone(row.health)}">${escapeHtml(row.health)}</span>
          <span data-tone="quiet">${escapeHtml(row.mutability)}</span>
        </span>
      </summary>
      <div class="mechanics-row-body">
        <dl>
          <div><dt>Default</dt><dd>${escapeHtml(row.defaultValue)}</dd></div>
          <div><dt>Scope</dt><dd>${escapeHtml(row.scope)}</dd></div>
          <div><dt>Owner</dt><dd>${escapeHtml(row.owner)}</dd></div>
          <div><dt>Apply</dt><dd>${escapeHtml(row.apply)}</dd></div>
          <div><dt>Secrecy</dt><dd>${escapeHtml(row.secrecy)}</dd></div>
        </dl>
        <p><strong>Consequence.</strong> ${escapeHtml(row.consequence)}</p>
      </div>
    </details>`;
}

function renderHouseMechanics() {
  const categoryButtons = [
    { id: "all", label: "All", count: HOUSE_MECHANICS_SNAPSHOT.categories.reduce((sum, category) => sum + category.rows.length, 0) },
    ...HOUSE_MECHANICS_SNAPSHOT.categories.map(category => ({ id: category.id, label: category.label, count: category.rows.length }))
  ];
  return `
    <section class="mechanics-observatory" aria-labelledby="mechanics-title">
      <header class="mechanics-lead">
        <span class="eyebrow">House mechanics</span>
        <h2 id="mechanics-title">Mechanical observatory</h2>
        <p>Effective values, ownership, health, and consequence from the current source census.</p>
        <div class="mechanics-snapshot-status" aria-label="Snapshot status">
          <span data-tone="attention">${escapeHtml(HOUSE_MECHANICS_SNAPSHOT.connection)}</span>
          <span>Source census · ${escapeHtml(HOUSE_MECHANICS_SNAPSHOT.capturedAt)}</span>
          <span>${escapeHtml(HOUSE_MECHANICS_SNAPSHOT.revision)}</span>
        </div>
      </header>
      <div class="mechanics-controls">
        <label class="mechanics-search">
          <span>Search every mechanism</span>
          <input type="search" data-mechanics-search value="${escapeHtml(state.mechanicsQuery)}" placeholder="Recall, timeout, Hallway, backup…" autocomplete="off">
        </label>
        <nav class="mechanics-categories" aria-label="Mechanical observatory categories">
          ${categoryButtons.map(category => `
            <button type="button" data-mechanics-category="${escapeHtml(category.id)}" aria-pressed="${String(state.mechanicsCategory === category.id)}">
              <span>${escapeHtml(category.label)}</span><small>${category.count}</small>
            </button>`).join("")}
        </nav>
      </div>
      <p class="mechanics-results-status" role="status" aria-live="polite"></p>
      <div class="mechanics-results"></div>
      <footer>Disconnected surface · PostgreSQL-backed controls may be Host-writable later; every control remains read-only here.</footer>
    </section>`;
}

function renderMechanicsResults() {
  const observatory = timeline.querySelector(".mechanics-observatory");
  if (!observatory) return;
  const entries = mechanicsEntries();
  const query = state.mechanicsQuery.trim();
  const category = HOUSE_MECHANICS_SNAPSHOT.categories.find(candidate => candidate.id === state.mechanicsCategory);
  observatory.querySelectorAll("[data-mechanics-category]").forEach(button => {
    button.setAttribute("aria-pressed", String(button.dataset.mechanicsCategory === state.mechanicsCategory));
  });
  observatory.querySelector(".mechanics-results-status").textContent = query
    ? `${entries.length} mechanism${entries.length === 1 ? "" : "s"} across all categories for “${query}”`
    : `${entries.length} mechanism${entries.length === 1 ? "" : "s"} · ${category?.label ?? "All categories"}`;
  observatory.querySelector(".mechanics-results").innerHTML = entries.length > 0
    ? entries.map(renderMechanicRow).join("")
    : '<div class="mechanics-empty"><strong>No mechanism matches that search.</strong><span>Try Recall, timeout, Hallway, room, or backup.</span></div>';
}

function renderCardPicker(verb, attr, options) {
  if (options.length === 0) return "";
  const picker = state.accessPicker
    ? `<div class="card-picker">${options.map(option => `<button type="button" ${attr}="${escapeHtml(option)}">${escapeHtml(option)}</button>`).join("")}</div>`
    : "";
  return `
    <div class="access-control">
      <button class="card-verb" type="button" data-access-picker aria-expanded="${String(state.accessPicker)}">${escapeHtml(verb)}</button>
      ${picker}
    </div>`;
}

function renderCollectionDoor() {
  if (state.mode !== "direct") return "";
  if (!state.newSpiritOpen) return '<button class="collection-door" type="button" data-new-spirit>New spirit</button>';
  return `
    <form class="spirit-form" data-spirit-form>
      <input type="text" name="spirit-name" placeholder="Spirit name…" aria-label="New spirit name" maxlength="24">
      <button class="card-verb" type="submit">Welcome</button>
    </form>`;
}

function renderConversationList() {
  if (state.mode === "house") {
    conversationList.innerHTML = '<div class="house-scope-note"><strong>House scope</strong><span>Shared mechanics, memory, lessons, and status.</span></div>';
    return;
  }
  conversationList.innerHTML = visibleConversations()
    .map(item => renderSubjectRow(item, item.id === state.activeId, itemIsLive(item)))
    .join("") + renderCollectionDoor();
  if (state.newSpiritOpen) window.requestAnimationFrame(() => conversationList.querySelector(".spirit-form input")?.focus({ preventScroll: true }));
}


function renderHeader(item) {
  header.dataset.kind = item.kind;
  header.innerHTML = `
    ${renderAvatar(item.glyph, "lg")}
    <div class="subject-header-copy">
      <h1 class="subject-heading">${escapeHtml(item.name)}</h1>
      <p>${escapeHtml(item.subtitle)}</p>
    </div>
    ${renderHeaderContext(item)}
  `;
}

function renderThreadMeta(item) {
  const record = hallwayRecords[item.hallwayId];
  return `
    <div class="thread-header-meta">
      <span class="thread-date">${escapeHtml(item.date)}</span>
      <button class="inspector-door hallway-record-door" type="button" data-hallway-record aria-label="Open the ${escapeHtml(record.name)} record">${escapeHtml(record.name)}</button>
    </div>
    ${item.endState === "open" ? `
      <div class="hallway-statuses" aria-label="Live channels">
        <span class="hallway-status">Connection: ${escapeHtml(item.connection)}</span>
        <span class="hallway-status">Delivery: ${escapeHtml(item.delivery)}</span>
      </div>
    ` : `<p class="thread-header-seal">${escapeHtml(item.sealLine)}</p>`}
  `;
}

function renderHeaderChip(text) {
  return `<span class="header-chip">${escapeHtml(text)}</span>`;
}

function renderHeaderVerb(label, verb) {
  return `<button class="header-verb" type="button" data-header-verb="${escapeHtml(verb)}">${escapeHtml(label)}</button>`;
}

// one owner for the header's contextual slot: the view's own verb, else its quietest fact
function renderHeaderContext(item) {
  if (state.activeView === "live") {
    if (item.kind === "hallway") return renderThreadMeta(item);
    return renderSessionControl(item);
  }
  if (state.activeView === "state") {
    if (item.kind === "house") return renderHeaderVerb("Local interface settings", "settings");
    if (item.kind === "direct") return renderHeaderChip(item.body);
    if (item.kind === "hallway") return renderHeaderChip(item.endState === "open" ? item.connection : THREAD_STATE_LABELS[item.endState]);
    return renderHeaderChip(projectSurfaces[item.id].workState);
  }
  if (item.kind === "direct") return renderHeaderVerb("Record memory", "memory");
  if (item.kind === "hallway") return item.endState === "open" ? renderHeaderVerb("Seal gathering", "seal") : renderHeaderChip(item.sealLine);
  if (item.kind === "project") return renderHeaderChip(`${projectSurfaces[item.id].evidence.length} receipts`);
  return renderHeaderChip(`${houseSediment().length} entries`);
}

function sessionToggleLabel(item) {
  if (item.kind === "direct") return activeSession(item).label;
  return activeProjectSession(item)?.label ?? "Linked sessions";
}

// the picker's rows are the session history; only Direct and Projects own sessions
function renderSessionControl(item) {
  if (item.kind !== "direct" && item.kind !== "project") return "";
  const menu = state.sessionMenuOpen ? `<div class="session-menu">${renderSessionMenu(item)}</div>` : "";
  return `
    <div class="session-control">
      <button class="session-toggle" type="button" data-session-toggle aria-expanded="${String(state.sessionMenuOpen)}" aria-label="Sessions for ${escapeHtml(item.name)}">${escapeHtml(sessionToggleLabel(item))}<span aria-hidden="true"> ▾</span></button>
      ${menu}
    </div>
  `;
}

function renderSessionMenu(item) {
  if (item.kind === "project") return renderProjectSessionList(item);
  return `
    <button class="session-menu-action" type="button" data-new-session>New session</button>
    ${item.sessions.map(session => renderDirectSessionRow(session, session.id === item.activeSessionId)).join("")}
  `;
}

function renderTimeline(item) {
  const itemMessages = activeMessages(item);
  const firstLiveIndex = itemMessages.findIndex(message => message.live);
  const messages = itemMessages.map((message, index) => {
    const boundary = index === firstLiveIndex && item.liveBoundary
      ? `<div class="live-boundary" role="separator">${escapeHtml(item.liveBoundary)} · live updates begin</div>`
      : "";
    return boundary + renderMessage(message, index, state.selectedMessageIndex === index);
  }).join("");
  const action = item.kind === "hallway" && item.action ? renderActionEvent(item.action) : "";
  const recallEvent = item.recallEvent ? renderRecallEvent(item.recallEvent) : "";
  const seal = item.kind === "hallway" && item.endState !== "open"
    ? `<div class="thread-seal" role="separator">${escapeHtml(item.sealLine)}</div>`
    : "";
  let emptyState = "";
  if (itemMessages.length === 0) {
    let headline = "No messages here yet.";
    let reason = "Nothing has been delivered here.";
    if (item.kind === "direct") {
      headline = "This session is clean.";
      reason = `Start the new thread with ${item.name} here.`;
    } else if (item.kind === "hallway") {
      headline = "No messages in this gathering.";
      if (item.endState !== "open") reason = item.sendReason;
    }
    const staleReason = item.connection === "Stale" ? "Stale means there is no current live update stream." : null;
    emptyState = renderEmptyState(headline, [reason, staleReason]);
  }

  timeline.innerHTML = emptyState + messages + action + recallEvent + seal;
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
        ${renderFactList([
          ["Intent", action.intent],
          ["Arguments", action.arguments],
          ["Result", action.result],
          ["Evidence", action.evidence],
          ["Authority", action.authority],
          ["Context effect", action.contextEffect],
          ["Durability", action.durability],
          ["Changed files", action.changedFiles]
        ])}
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
  state.sessionMenuOpen = false;
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
  state.activeView = "live";
  state.sessionMenuOpen = false;
  input.value = "";
  input.style.height = "auto";
  updateComposerState();
  render();
}

function renderProjectSessionList(item) {
  const project = projectSurfaces[item.id];
  return `
    <section class="project-session-list" aria-label="${escapeHtml(item.name)} linked sessions">
      ${project.sessions.map(link => renderProjectSessionRow(link, project.activeSessionId === link.id)).join("")}
    </section>
  `;
}

function renderProjectConversation(item) {
  const session = activeProjectSession(item);
  renderTimeline(item);
  timeline.insertAdjacentHTML("afterbegin", `
    <section class="project-conversation-banner">
      <button type="button" data-close-project-session>← Project overview</button>
      <div><span class="project-route-type">${escapeHtml(session.routeType)}</span><h2>${escapeHtml(session.label)}</h2><p>${escapeHtml(session.activity)} · scoped to ${escapeHtml(item.name)}</p></div>
      <span>${escapeHtml(session.state)}</span>
    </section>
  `);
}

function renderProjectSurface(item) {
  const project = projectSurfaces[item.id];
  const rooms = `<div class="project-room-list">${project.rooms.map(room => `<span>${escapeHtml(room)}</span>`).join("")}</div>`;
  const views = {
    live: `
      <div class="overview-grid">
        ${renderOverviewHero("Project overview", item.name, item.description, {
          label: "Work state",
          title: project.workState,
          detail: project.workDetail
        })}
        <section class="specimen-card"><h3>Involved rooms</h3>${rooms}</section>
        <section class="specimen-card"><h3>Linked sessions</h3><p>${project.sessions.length} scoped conversations · each uses Direct message or Hallway participation without leaving the project. The header picker opens them.</p></section>
      </div>`,
    state: `
      <div class="specimen-stack">
        ${renderSpecimenLead("Project state", item.name)}
        <section class="state-section">
          <span class="eyebrow">Work state</span>
          ${renderFactList([
            ["State", project.workState],
            ["Status", item.status],
            ["Recall policy", item.recall],
            ["Linked sessions", project.sessions.length]
          ])}
          <p>${escapeHtml(project.workDetail)}</p>
        </section>
        ${project.sessions.map(link => `
        <section class="state-section">
          <span class="eyebrow">${escapeHtml(link.label)}</span>
          ${renderFactList([
            ["Route", link.routeType],
            ["State", link.state],
            ["Activity", link.activity]
          ])}
        </section>`).join("")}
        <section class="state-section">
          <span class="eyebrow">Host</span>
          ${renderFactList([
            ["Connection", "Offline"],
            ["Live state", "Unavailable"]
          ])}
        </section>
        <section class="state-section">
          <span class="eyebrow">Evidence health</span>
          ${renderFactList([
            ["Listed", project.evidence.length],
            ["Durable receipts", "Unavailable here"]
          ])}
        </section>
        <section class="state-section">
          <span class="eyebrow">Involved rooms</span>
          ${rooms}
          ${renderCardPicker("Involve room", "data-involve-room", PROJECT_ROOMS.filter(room => !project.rooms.includes(room)))}
        </section>
        <section class="state-section">
          <span class="eyebrow">Activity</span>
          ${project.activity.map(renderHistoryEvent).join("")}
        </section>
      </div>`,
    durable: `
      <div class="specimen-stack">
        ${renderSpecimenLead("Project evidence", item.name)}
        ${[...project.evidence]
          .sort((left, right) => right.date.localeCompare(left.date))
          .map(entry => renderDurableEntry({
            date: entry.date,
            title: entry.title,
            mark: "Evidence",
            detail: `${entry.result} · ${entry.authority}`
          })).join("")}
      </div>`
  };
  timeline.innerHTML = views[state.activeView];
  timeline.scrollTop = 0;
}

function houseSediment() {
  const memories = houseSurface.memories.map(entry => ({
    date: entry.date,
    title: entry.title,
    mark: "Memory",
    detail: `#${entry.id} · ${entry.scope}`,
    library: { owner: "house", type: "memory", id: entry.id }
  }));
  const lessons = houseSurface.lessons.map(entry => ({
    date: entry.date,
    title: entry.title,
    mark: "Lesson",
    detail: `#${entry.id} · ${entry.kind}`,
    library: { owner: "house", type: "lesson", id: entry.id }
  }));
  return [...memories, ...lessons].sort((left, right) => right.date.localeCompare(left.date));
}

function renderHouseSurface() {
  const views = {
    live: `
      <div class="overview-grid">
        ${renderOverviewHero("House overview", "Solarisael House", "Shared mechanics, memories, lessons, and substrate state. Spirit-room memory stays inside its room.")}
        <section class="specimen-card"><h3>House record</h3><p>${houseSurface.memories.length} memories and ${houseSurface.lessons.length} lessons on the shared shelves.</p><button type="button" data-open-subject-view="durable">Open the record</button></section>
        <section class="specimen-card"><h3>Mechanical observatory</h3><p>${HOUSE_MECHANICS_SNAPSHOT.categories.length} categories with effective values, ownership, health, and consequence.</p><button type="button" data-open-subject-view="state">Open mechanics</button></section>
        <section class="specimen-card"><h3>Ownership</h3><p>House commons do not absorb Kintsu, Kodo, or Tuner room memory.</p></section>
      </div>`,
    state: renderHouseMechanics(),
    durable: `
      <div class="specimen-stack house-record-stack">
        ${renderSpecimenLead("House record", "Memories & Lessons")}
        ${houseSediment().map(renderDurableEntry).join("")}
      </div>`
  };
  timeline.innerHTML = views[state.activeView];
  if (state.activeView === "state") renderMechanicsResults();
  timeline.scrollTop = 0;
}

function renderRoomMemories(item) {
  const memories = [...(roomMemoryShelves[item.id] ?? [])].sort((left, right) => right.date.localeCompare(left.date));
  const entries = memories.length === 0
    ? renderEmptyState(`No ${item.name} room memories available.`)
    : memories.map(entry => renderDurableEntry({
        date: entry.date,
        title: entry.title,
        mark: "Memory",
        detail: `#${entry.id} · ${entry.detail}`,
        library: { owner: item.id, type: "memory", id: entry.id }
      })).join("");
  timeline.innerHTML = `
    <div class="specimen-stack">
      ${renderSpecimenLead("Room record", item.name)}
      ${entries}
    </div>`;
  timeline.scrollTop = 0;
}

function renderSubjectView(item) {
  if (item.kind === "house") {
    renderHouseSurface();
    return;
  }
  if (item.kind === "project") {
    if (state.activeView === "live" && activeProjectSession(item)) renderProjectConversation(item);
    else renderProjectSurface(item);
    return;
  }
  if (state.activeView === "live") {
    renderTimeline(item);
    return;
  }
  if (state.activeView === "durable") {
    if (item.kind === "hallway") renderHallwayRecordView(item);
    else renderRoomMemories(item);
    return;
  }
  renderSubjectState(item);
}

function renderMemberState(presence) {
  return `
    <section class="state-section">
      <span class="eyebrow">${escapeHtml(`${presence.spirit} · ${presence.session}`)}</span>
      ${renderFactList([
        ["Liveness", `${presence.liveness} · ${presence.activity}`],
        ["Read position", presence.readPosition],
        ["Context used", presence.contextUsed],
        ["Compaction", presence.compaction],
        ["Recall", presence.recall],
        ["Evidence", presence.evidence]
      ])}
    </section>`;
}

// a real recall replayed as specimen: the 2026-08-17 20:12 turn, values verbatim from its receipt
const TURN_RECALL = {
  query: "Athanor GUI prototype three-layer slot grammar variables shelf",
  transport: "rust-postgres",
  matched: "1 canon · 5 memories",
  canon: { name: "The Athanor", type: "project", line: "The public platform that creates and runs Houses — Solarisael House is the reference implementation." },
  candidates: [
    { id: "3660", title: "Evening third wave: hints die, members get context, the header learns verbs", score: "1.69", coverage: 60, scope: "kodo", heading: "__preamble__" },
    { id: "3557", title: "The first component-first Athanor GUI prototype is alive in grayscale HTML", score: "1.08", coverage: 50, scope: "house", heading: "__preamble__" },
    { id: "3584", title: "The Athanor GUI gained local lawbooks and paid Tuner's conformance debt", score: "0.90", coverage: 40, scope: "house", heading: "__preamble__" },
    { id: "3647", title: "Second wave: the three-layer slot grammar — live/state/durable, chart-ruled", score: "0.66", coverage: 50, scope: "kodo", heading: "## The ruling" },
    { id: "3639", title: "GUI critique morning: the map that took four drawings, nine wounds ruled", score: "0.62", coverage: 50, scope: "kodo", heading: "## The navigation map" }
  ],
  resonance: [
    { label: "The day Sol and Kintsu gave The Athanor its future anatomy", activation: 0.29, members: 73 },
    { label: "2026-07-02 website day: deploy machinery, cathedral lintel", activation: 0.25, members: 90 },
    { label: "Kodo Memory - 2026-05-12 (afternoon → evening)", activation: 0.23, members: 121 }
  ]
};
TURN_RECALL.time = "20:12";
TURN_RECALL.duration = "22.2 s";
conversations.kintsu.recallEvent = TURN_RECALL;


function renderTermMeter(coverage) {
  return `<span class="term-meter" role="img" aria-label="${coverage}% terms matched"><span style="width: ${coverage}%"></span></span>`;
}

function renderRecallCandidate(candidate) {
  return `
    <article class="recall-candidate surface-row">
      <div class="recall-candidate-head">
        <strong>#${escapeHtml(candidate.id)} ${escapeHtml(candidate.title)}</strong>
        <span class="recall-scope">${escapeHtml(candidate.scope)}</span>
      </div>
      <div class="recall-candidate-score">score ${escapeHtml(candidate.score)} ${renderTermMeter(candidate.coverage)} ${candidate.coverage}% terms</div>
      <small>${escapeHtml(candidate.heading)}</small>
    </article>`;
}

function renderRecallCard(recall) {
  return `
    <p class="recall-query">"${escapeHtml(recall.query)}"</p>
    ${renderFactList([
      ["Transport", recall.transport],
      ["Matched", recall.matched]
    ])}
    <article class="recall-canon">
      <strong>◆ canon · ${escapeHtml(recall.canon.name)} · ${escapeHtml(recall.canon.type)}</strong>
      <small>${escapeHtml(recall.canon.line)}</small>
    </article>
    ${recall.candidates.map(renderRecallCandidate).join("")}
    <section class="recall-resonance">
      <span class="eyebrow">Cluster resonance</span>
      ${renderFactList(recall.resonance.map(cluster => [cluster.label, `${cluster.activation.toFixed(2)} · ${cluster.members} members`]))}
    </section>
    <p>Replayed receipt · a live turn feed needs the Host.</p>`;
}

function renderRecallEvent(recall) {
  return `
    <details class="action-event recall-event">
      <summary>
        <span class="action-verb">⌕ recall</span>
        <span class="action-target">"${escapeHtml(recall.query)}"</span>
        <span class="action-state">${escapeHtml(`${recall.matched} · ${recall.time} · ${recall.duration}`)}</span>
      </summary>
      <div class="action-details">${renderRecallCard(recall)}</div>
    </details>`;
}

function renderSubjectState(item) {
  const threadEnded = item.kind === "hallway" && item.endState !== "open";
  const liveChannels = item.kind === "hallway"
    ? `
      <section class="state-section">
        <span class="eyebrow">Live channels</span>
        ${renderFactList([
          ["Connection", item.connection ?? "Ended"],
          ["Delivery", item.delivery ?? "No delivery in an ended gathering"],
          ["Thread state", THREAD_STATE_LABELS[item.endState]],
          ["Read boundary", item.liveBoundary ?? item.sealLine]
        ])}
      </section>`
    : "";
  const memberState = item.kind === "hallway"
    ? (threadEnded
      ? `
      <section class="state-section">
        <span class="eyebrow">Participants</span>
        ${renderFactList([
          ["Spoke here", item.participants.join(" · ")],
          ["Closed", item.sealLine]
        ])}
      </section>`
      : (item.presences ?? []).map(renderMemberState).join(""))
    : `
      <section class="state-section">
        <span class="eyebrow">Runtime</span>
        ${renderFactList([
          ["Room", item.room],
          ["Spirit", item.name],
          ["Body", item.body],
          ["Liveness", item.status]
        ])}
      </section>
      <section class="state-section">
        <span class="eyebrow">Attention</span>
        ${renderFactList([
          ["Context used", "6k of 32k"],
          ["Compaction", "Not needed"],
          ["Recall policy", item.recall],
          ["Evidence", "No live receipt"]
        ])}
      </section>`;
  timeline.innerHTML = `
    <div class="specimen-stack">
      ${renderSpecimenLead("State of", item.name)}
      ${liveChannels}
      ${memberState}
      <section class="state-section">
        <span class="eyebrow">Active instructions</span>
        <ul class="plain-list">
          <li>Room identity and active spirit</li>
          <li>Nearest GUI lessons map</li>
          <li>Current operator request</li>
        </ul>
      </section>
      <section class="state-section">
        <span class="eyebrow">Recall & AKASHA</span>
        ${renderFactList([
          ["Policy", item.recall],
          ["Transport", "Offline"],
          ["Last receipt", "Unavailable"]
        ])}
      </section>
      <section class="state-section">
        <span class="eyebrow">Active lessons</span>
        <ul class="plain-list">
          <li>#316 · preserve subject authority</li>
          <li>#322 · fixed refusal copy</li>
          <li>#340 · bounded visual proof</li>
        </ul>
      </section>
      <section class="state-section">
        <span class="eyebrow">Striatum</span>
        ${renderFactList([
          ["Firing", "None observed"],
          ["Effect", "No current receipt"]
        ])}
      </section>
      <section class="state-section">
        <span class="eyebrow">GIGA</span>
        ${renderFactList([
          ["Flagged", "2 candidates"],
          ["Authority", "Proposals only"],
          ["Review", "Unreviewed"]
        ])}
      </section>
    </div>`;
  timeline.scrollTop = 0;
}

function renderThreadRecordRow(thread, current) {
  const detail = thread.endState === "open" ? thread.subtitle : thread.sealLine;
  return `
    <button class="durable-entry surface-row" type="button" data-thread-id="${escapeHtml(thread.id)}" aria-current="${String(current)}">
      <time>${escapeHtml(thread.date)}</time>
      <strong>${escapeHtml(thread.name)}</strong>
      <small>${escapeHtml(`${THREAD_STATE_LABELS[thread.endState]} · ${detail}`)}</small>
    </button>
  `;
}

function renderHallwayRecordView(item) {
  const record = hallwayRecords[item.hallwayId];
  timeline.innerHTML = `
    <div class="specimen-stack">
      <section class="membership-card">
        <h2>${escapeHtml(record.name)}</h2>
        ${renderFactList([
          ["Authority", record.authority],
          ["Access", record.access],
          ["Membership", record.membership.join(" · ")],
          ...(record.invited?.length ? [["Invited", record.invited.map(room => `${room} · may see and enter · has not entered`).join(" · ")]] : [])
        ])}
        ${renderCardPicker("Extend access", "data-invite-room", HOUSE_ROOMS.filter(room => !record.membership.includes(room) && !(record.invited ?? []).includes(room)))}
      </section>
      ${record.threads.map(id => renderThreadRecordRow(conversations[id], id === item.id)).join("")}
    </div>`;
  timeline.scrollTop = 0;
}

function renderMemberDock(item) {
  if (item.kind !== "hallway") {
    memberDock.hidden = true;
    memberDock.innerHTML = "";
    return;
  }
  const presences = item.presences ?? [];
  memberDock.hidden = false;
  memberDock.innerHTML = `
    <span class="eyebrow">Members</span>
    ${presences.length === 0
      ? renderEmptyState("No one is live in this gathering.", [item.sealLine])
      : Object.entries(Object.groupBy(presences, presence => presence.spirit)).map(([spirit, group]) => `
          <section class="member-group">
            <strong>${escapeHtml(spirit)}</strong>
            ${group.map(presence => renderPresenceRow(presence, state.selectedPresenceId === presence.id)).join("")}
          </section>
        `).join("")}
  `;
}

function renderPresenceProfile(item) {
  const presence = item.presences?.find(candidate => candidate.id === state.selectedPresenceId);
  if (!presence || !state.profileAnchor) {
    profileLayer.hidden = true;
    profileLayer.innerHTML = "";
    return;
  }
  const anchor = state.profileAnchor;
  profileLayer.style.setProperty("--profile-top", `${Math.max(56, anchor.top)}px`);
  profileLayer.style.setProperty("--profile-left", `${Math.max(12, anchor.left - 330)}px`);
  profileLayer.hidden = false;
  profileLayer.innerHTML = `
    <section class="presence-profile" role="dialog" aria-label="${escapeHtml(presence.spirit)} ${escapeHtml(presence.session)} profile">
      <header>
        <span class="profile-glyph" aria-hidden="true">${escapeHtml(presence.glyph)}</span>
        <div><strong>${escapeHtml(presence.spirit)}</strong><span>${escapeHtml(presence.session)} · ${escapeHtml(presence.liveness)}</span></div>
        <button type="button" data-close-profile aria-label="Close profile">×</button>
      </header>
      ${renderFactList([
        ["Room", presence.room],
        ["Session ID", presence.sessionId],
        ["Permission", presence.permission],
        ["Activity", presence.activity],
        ["Read position", presence.readPosition],
        ["Context used", presence.contextUsed],
        ["Compaction", presence.compaction],
        ["Recall", presence.recall],
        ["Evidence", presence.evidence]
      ])}
    </section>
  `;
}

function renderInspector(item) {
  const selectedMessage = state.selectedMessageIndex === null ? null : activeMessages(item)[state.selectedMessageIndex];
  let selection = "";
  if (selectedMessage) {
    const authority = selectedMessage.local ? "Local · undelivered" : "Displayed snapshot";
    selection = `
      <section class="context-card context-selection">
        <h3>Current selection</h3>
        ${renderFactList([
          ["Author", selectedMessage.author],
          ["Time", selectedMessage.time],
          ["Authority", authority]
        ])}
      </section>`;
  }

  let eyebrow = "Room shelf";
  if (item.kind === "house") eyebrow = "House shelf";
  else if (item.kind === "project") eyebrow = "Project shelf";
  inspectorEyebrow.textContent = eyebrow;

  const librarySelection = state.librarySelection?.owner === item.id ? state.librarySelection : null;
  if (librarySelection) {
    let entry;
    let scope;
    let scopeLabel = "Scope";
    let selectionLabel = "Selected memory";
    if (librarySelection.type === "lesson") {
      entry = houseSurface.lessons.find(candidate => candidate.id === librarySelection.id);
      scope = entry?.kind;
      scopeLabel = "Kind";
      selectionLabel = "Selected lesson";
    } else {
      const memories = item.kind === "house" ? houseSurface.memories : roomMemoryShelves[item.id] ?? [];
      entry = memories.find(candidate => candidate.id === librarySelection.id);
      scope = item.kind === "house" ? entry?.scope : `${item.name} room`;
    }
    if (entry) {
      inspectorTitle.textContent = `#${entry.id}`;
      inspectorContent.innerHTML = `
        <section class="context-card context-library-selection">
          <h3>${selectionLabel}</h3>
          <strong>${escapeHtml(entry.title)}</strong>
          ${renderFactList([
            [scopeLabel, scope],
            ["Authority here", "Local-only metadata · PostgreSQL body unavailable"]
          ])}
          <button type="button" data-clear-library-selection>Clear selection</button>
        </section>`;
      return;
    }
  }

  if (item.kind === "house") {
    inspectorTitle.textContent = "Solarisael House";
    inspectorContent.innerHTML = `
      <section class="context-card"><h3>House pulse</h3>${renderFactList([
        ["Memories", houseSurface.memories.length],
        ["Lessons", houseSurface.lessons.length],
        ["Mechanics", HOUSE_MECHANICS_SNAPSHOT.categories.reduce((sum, category) => sum + category.rows.length, 0)],
        ["Host", HOUSE_MECHANICS_SNAPSHOT.connection]
      ])}</section>
      ${renderInspectorDoors("Open", [
        ["state", `Mechanics · ${HOUSE_MECHANICS_SNAPSHOT.categories.length} categories`],
        ["durable", `Memories & Lessons · ${houseSurface.memories.length + houseSurface.lessons.length}`]
      ])}`;
    return;
  }

  if (item.kind === "project") {
    const project = projectSurfaces[item.id];
    const projectSession = activeProjectSession(item);
    const conversation = projectSession
      ? `<span class="project-route-type">${escapeHtml(projectSession.routeType)}</span><strong>${escapeHtml(projectSession.label)}</strong><p>${escapeHtml(projectSession.state)} · ${projectSession.messages.length} messages</p>`
      : "<p>No project session selected.</p>";
    inspectorTitle.textContent = item.name;
    inspectorContent.innerHTML = `
      <section class="context-card"><h3>Work state</h3><strong>${escapeHtml(project.workState)}</strong><p>${escapeHtml(project.workDetail)}</p></section>
      <section class="context-card"><h3>Project conversation</h3>${conversation}</section>
      <section class="context-card"><h3>Project shelf</h3>${renderFactList([
        ["Sessions", project.sessions.length],
        ["Rooms", project.rooms.length],
        ["Evidence", project.evidence.length]
      ])}</section>
      ${renderInspectorDoors("Open", [
        ["state", "Status"],
        ["durable", `Evidence · ${project.evidence.length}`]
      ])}
      ${selection}`;
    return;
  }

  if (item.kind === "direct") {
    const session = activeSession(item);
    const memories = roomMemoryShelves[item.id] ?? [];
    inspectorTitle.textContent = `${item.name} room`;
    inspectorContent.innerHTML = `
      <section class="context-card"><h3>Current session</h3><strong>${escapeHtml(session.label)}</strong><p>${escapeHtml(session.id)} · ${escapeHtml(session.state)} · ${session.messages.length} messages</p></section>
      <section class="context-card"><h3>Room continuity</h3>${renderFactList([
        ["Memories", memories.length],
        ["Recall", item.recall],
        ["Body", item.body]
      ])}</section>
      ${renderInspectorDoors("Navigate", [
        ["state", "Status"],
        ["durable", `Memories · ${memories.length}`]
      ])}
      ${selection}`;
    return;
  }

  inspectorTitle.textContent = item.name;
  inspectorContent.innerHTML = `
    <section class="context-card"><h3>Hallway state</h3>${renderFactList([
      ["Room", item.room],
      ["Status", item.connection ?? item.status],
      ["Recall", item.recall]
    ])}</section>
    ${selection}`;
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

function syncMobileSidebarAccessibility() {
  const concealed = mobileLayout.matches && shell.dataset.sidebarOpen !== "true";
  sidebar.inert = concealed;
  sidebar.setAttribute("aria-hidden", String(concealed));
}

function setMobileSidebarOpen(open) {
  shell.dataset.sidebarOpen = String(open);
  sidebarToggle.setAttribute("aria-pressed", String(open));
  syncMobileSidebarAccessibility();
}

function closeMobileSidebar() {
  drawerFocusGeneration += 1;
  setMobileSidebarOpen(false);
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



function activeInstrument(item) {
  if (item.kind === "house") {
    return state.activeView === "live" ? "overview" : "work";
  }
  if (item.kind === "project" && state.activeView === "live") {
    return activeProjectSession(item) ? "chat" : "overview";
  }
  return state.activeView === "live" ? "chat" : "work";
}


function subjectViewLabels(item) {
  const labels = [...SUBJECT_VIEW_LABELS[item.kind]];
  if (item.kind === "project" && activeProjectSession(item)) {
    labels[0] = "Conversation";
  }
  return labels;
}


function render() {
  const item = conversations[state.activeId];
  const instrument = activeInstrument(item);
  state.mode = item.kind;
  shell.dataset.activeId = item.id;
  shell.dataset.subjectKind = item.kind;
  shell.dataset.instrument = instrument;
  shell.dataset.activeSessionId = activeSession(item)?.id ?? "";
  shell.dataset.activeProjectSessionId = activeProjectSession(item)?.id ?? "";
  shell.dataset.selectedPresenceId = state.selectedPresenceId ?? "";
  shell.dataset.hallway = String(item.kind === "hallway");
  shell.dataset.density = state.density;
  shell.dataset.kintsuActiveSessionId = conversations.kintsu.activeSessionId;
  shell.dataset.kodoActiveSessionId = conversations.kodo.activeSessionId;
  shell.dataset.textScale = state.textScale;
  shell.dataset.measure = state.measure;
  shell.dataset.contrast = state.contrast;
  shell.dataset.reducedMotion = String(state.reducedMotion);
  shell.dataset.timestamps = String(state.timestamps);
  shell.dataset.statusVisible = String(state.statusVisible);
  inspectorToggle.hidden = item.kind === "hallway";
  houseDoor.setAttribute("aria-pressed", String(item.kind === "house"));
  document.querySelectorAll(".mode-button").forEach(button => {
    const active = button.dataset.mode === state.mode;
    button.classList.toggle("is-active", active);
    button.setAttribute("aria-pressed", String(active));
  });
  const viewLabels = subjectViewLabels(item);
  let activeViewButton = null;
  subjectViewButtons.forEach((button, index) => {
    const active = button.dataset.subjectView === state.activeView;
    button.classList.toggle("is-active", active);
    button.setAttribute("aria-pressed", String(active));
    button.querySelector("[data-view-label]").textContent = viewLabels[index];
    if (active) activeViewButton = button;
  });
  composer.hidden = instrument !== "chat";
  const activeViewIndex = SUBJECT_VIEWS.indexOf(state.activeView);
  timeline.setAttribute("aria-label", item.kind === "house" || item.kind === "project" ? `${item.name} ${viewLabels[activeViewIndex]} view` : state.activeView === "live" ? "Message timeline" : `${item.name} ${viewLabels[activeViewIndex]} view`);
  renderConversationList();
  renderHeader(item);
  renderSubjectView(item);
  renderInspector(item);
  renderMemberDock(item);
  renderPresenceProfile(item);
  renderBellToggle();
  syncMobileSidebarAccessibility();
  window.requestAnimationFrame(() => revealActiveViewButton(activeViewButton));
}

function leaveHouse() {
  const previous = state.houseReturn;
  if (!previous) return;
  state.houseReturn = null;
  state.activeView = previous.activeView;
  openConversation(previous.activeId);
  window.requestAnimationFrame(() => houseDoor.focus({ preventScroll: true }));
}

function toggleHouse() {
  if (state.activeId === "house") {
    leaveHouse();
    return;
  }
  state.houseReturn = { mode: state.mode, activeId: state.activeId, activeView: state.activeView };
  state.activeView = "live";
  openConversation("house");
}

function extendHallwayAccess(item, room) {
  const record = hallwayRecords[item.hallwayId];
  (record.invited ??= []).push(room);
  state.accessPicker = false;
  render();
}

function involveProjectRoom(item, room) {
  projectSurfaces[item.id].rooms.push(room);
  state.accessPicker = false;
  render();
}

function welcomeSpirit(name) {
  const clean = name.trim();
  if (!clean) return;
  const id = clean.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/(^-|-$)/g, "") || `spirit-${conversationCount()}`;
  if (conversations[id]) {
    state.newSpiritOpen = false;
    openConversation(id);
    return;
  }
  conversations[id] = {
    id,
    kind: "direct",
    name: clean,
    glyph: clean[0].toUpperCase(),
    subtitle: `${clean}'s room · quiet`,
    listPreview: "No messages yet",
    updatedAt: "Now",
    room: id,
    status: "Quiet",
    recall: "Auto · Work",
    body: "Unassigned",
    messages: [],
    sessions: [{ id: `${id}-current`, label: "Current session", startedAt: "Today", state: "Open", messages: [] }],
    activeSessionId: `${id}-current`
  };
  state.newSpiritOpen = false;
  openConversation(id);
}

function conversationCount() {
  return Object.keys(conversations).length;
}

houseDoor.addEventListener("click", toggleHouse);
conversationList.addEventListener("click", event => {
  if (event.target.closest("[data-new-spirit]")) {
    state.newSpiritOpen = true;
    render();
    return;
  }
  const button = event.target.closest("[data-conversation]");
  if (button) openConversation(button.dataset.conversation);
});

conversationList.addEventListener("submit", event => {
  if (!event.target.closest("[data-spirit-form]")) return;
  event.preventDefault();
  welcomeSpirit(new FormData(event.target).get("spirit-name") ?? "");
});

conversationList.addEventListener("keydown", event => {
  if (event.key !== "Escape" || !event.target.closest("[data-spirit-form]")) return;
  event.stopPropagation();
  state.newSpiritOpen = false;
  render();
});


function openMode(mode) {
  if (state.activeId === "house") state.houseReturn = null;
  state.mode = mode;
  const first = visibleConversations()[0];
  if (first) openConversation(first.id);
}

document.querySelector(".mode-switcher").addEventListener("click", event => {
  const button = event.target.closest("[data-mode]");
  if (button) openMode(button.dataset.mode);
});

document.querySelector(".subject-views").addEventListener("click", event => {
  const button = event.target.closest("[data-subject-view]");
  if (!button) return;
  openSubjectView(button.dataset.subjectView);
});

inspectorContent.addEventListener("click", event => {
  if (event.target.closest("[data-clear-library-selection]")) {
    state.librarySelection = null;
    render();
    return;
  }
  const button = event.target.closest("[data-inspector-view]");
  if (!button) return;
  openSubjectView(button.dataset.inspectorView, { clearMessage: true });
});

function focusSessionToggle() {
  window.requestAnimationFrame(() => header.querySelector("[data-session-toggle]")?.focus({ preventScroll: true }));
}

function setSessionMenu(open, { restoreFocus = true } = {}) {
  state.sessionMenuOpen = open;
  render();
  if (restoreFocus) focusSessionToggle();
}

let localDraftSeq = 0;

function recordLocalMemory(item) {
  const shelf = roomMemoryShelves[item.id] ?? (roomMemoryShelves[item.id] = []);
  shelf.unshift({
    id: `draft-${++localDraftSeq}`,
    date: `${TODAY} ${new Date().toTimeString().slice(0, 5)}`,
    title: "Local memory draft",
    detail: "Recorded on this surface · local rehearsal, no durable write"
  });
  render();
}

function sealGathering(item) {
  item.endState = "sealed";
  item.sealLine = `Sealed by Sol · today ${new Date().toTimeString().slice(0, 5)}`;
  item.canPost = false;
  item.sendReason = "This gathering is sealed.";
  item.presences = [];
  state.selectedPresenceId = null;
  updateComposerState();
  render();
}

function openInterfaceSettings(trigger) {
  setDrawerView("settings", trigger);
  setMobileSidebarOpen(true);
}

header.addEventListener("click", event => {
  const item = conversations[state.activeId];
  const verb = event.target.closest("[data-header-verb]");
  if (verb) {
    if (verb.dataset.headerVerb === "memory") recordLocalMemory(item);
    if (verb.dataset.headerVerb === "seal") sealGathering(item);
    if (verb.dataset.headerVerb === "settings") openInterfaceSettings(verb);
    return;
  }
  if (event.target.closest("[data-hallway-record]")) {
    openSubjectView("durable");
    return;
  }
  if (event.target.closest("[data-session-toggle]")) {
    setSessionMenu(!state.sessionMenuOpen);
    return;
  }
  if (event.target.closest("[data-new-session]") && item.kind === "direct") {
    state.librarySelection = null;
    startDirectSession(item);
    focusSessionToggle();
    return;
  }
  const sessionRow = event.target.closest("[data-session-id]");
  if (sessionRow && item.kind === "direct") {
    openDirectSession(item, sessionRow.dataset.sessionId);
    focusSessionToggle();
    return;
  }
  const projectRow = event.target.closest("[data-project-session-id]");
  if (projectRow && item.kind === "project") {
    selectProjectSession(item, projectRow.dataset.projectSessionId);
    focusSessionToggle();
  }
});

// a click outside the picker dismisses it; clicks inside it are owned by the header listener
document.addEventListener("click", event => {
  if (!state.sessionMenuOpen) return;
  if (event.target.closest(".session-control")) return;
  setSessionMenu(false, { restoreFocus: false });
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

function selectProjectSession(projectItem, linkId) {
  const project = projectSurfaces[projectItem.id];
  const link = project.sessions.find(candidate => candidate.id === linkId);
  if (!link) return;
  const target = conversations[link.conversationId];
  const targetExists = link.routeType === "Direct message"
    ? target.sessions?.some(session => session.id === link.sessionId)
    : target.presences?.some(presence => presence.id === link.presenceId);
  if (!targetExists) return;
  state.drafts.set(draftKey(projectItem), input.value);
  project.activeSessionId = link.id;
  state.activeView = "live";
  state.sessionMenuOpen = false;
  state.selectedMessageIndex = null;
  input.value = state.drafts.get(draftKey(projectItem)) ?? "";
  input.style.height = "auto";
  updateComposerState();
  render();
}

function closeProjectSession(projectItem) {
  state.drafts.set(draftKey(projectItem), input.value);
  projectSurfaces[projectItem.id].activeSessionId = null;
  state.activeView = "live";
  state.selectedMessageIndex = null;
  input.value = state.drafts.get(projectItem.id) ?? "";
  input.style.height = "auto";
  updateComposerState();
  render();
}

timeline.addEventListener("click", event => {
  const item = conversations[state.activeId];
  const mechanicsCategory = event.target.closest("[data-mechanics-category]");
  if (mechanicsCategory) {
    state.mechanicsCategory = mechanicsCategory.dataset.mechanicsCategory;
    state.mechanicsQuery = "";
    const search = timeline.querySelector("[data-mechanics-search]");
    if (search) search.value = "";
    renderMechanicsResults();
    return;
  }
  const subjectDoor = event.target.closest("[data-open-subject-view]");
  if (subjectDoor) {
    openSubjectView(subjectDoor.dataset.openSubjectView);
    return;
  }
  if (event.target.closest("[data-access-picker]")) {
    state.accessPicker = !state.accessPicker;
    render();
    return;
  }
  const invite = event.target.closest("[data-invite-room]");
  if (invite) {
    extendHallwayAccess(item, invite.dataset.inviteRoom);
    return;
  }
  const involve = event.target.closest("[data-involve-room]");
  if (involve) {
    involveProjectRoom(item, involve.dataset.involveRoom);
    return;
  }
  const interfaceSettings = event.target.closest("[data-open-interface-settings]");
  if (interfaceSettings) {
    openInterfaceSettings(interfaceSettings);
    return;
  }
  const libraryCard = event.target.closest("[data-library-type][data-library-id]");
  if (libraryCard) {
    const next = { owner: state.activeId, type: libraryCard.dataset.libraryType, id: libraryCard.dataset.libraryId };
    const current = state.librarySelection;
    state.librarySelection = current?.owner === next.owner && current?.type === next.type && current?.id === next.id ? null : next;
    render();
    return;
  }
  const closeProject = event.target.closest("[data-close-project-session]");
  if (closeProject && item.kind === "project") {
    closeProjectSession(item);
    return;
  }
  const thread = event.target.closest("[data-thread-id]");
  if (thread) {
    openConversation(thread.dataset.threadId);
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

timeline.addEventListener("input", event => {
  const search = event.target.closest("[data-mechanics-search]");
  if (!search) return;
  state.mechanicsQuery = search.value;
  renderMechanicsResults();
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

composer.addEventListener("click", event => {
  const action = event.target.closest("[data-chat-action]");
  if (!action) return;
  const item = conversations[state.activeId];
  if (composerBlockReason(item) !== null) return;
  const message = action.dataset.chatAction === "sleep"
    ? "Paper boat prepared. Local rehearsal · no durable write."
    : "Memory draft prepared. Local rehearsal · no durable write.";
  state.continuityStatuses.set(draftKey(item), message);
  updateComposerState();
});

document.addEventListener("keydown", event => {
  if (!(event.ctrlKey || event.metaKey) || event.altKey || event.code !== "Space") return;
  event.preventDefault();
  if (state.switcherOpen) closeSwitcher();
  else openSwitcher();
});

switcherInput.addEventListener("input", () => {
  state.switcherQuery = switcherInput.value;
  state.switcherIndex = 0;
  renderSwitcher();
});

switcherInput.addEventListener("keydown", event => {
  if (event.key === "Tab") {
    event.preventDefault();
    return;
  }
  if (event.key === "Escape") {
    event.preventDefault();
    event.stopPropagation();
    closeSwitcher();
    return;
  }
  if (event.key === "ArrowDown" || event.key === "ArrowUp") {
    event.preventDefault();
    event.stopPropagation();
    if (switcherMatches.length === 0) return;
    const direction = event.key === "ArrowDown" ? 1 : -1;
    state.switcherIndex = (state.switcherIndex + direction + switcherMatches.length) % switcherMatches.length;
    renderSwitcher();
    return;
  }
  if (event.key === "Enter") {
    event.preventDefault();
    event.stopPropagation();
    const command = switcherMatches[state.switcherIndex];
    if (command) executeSwitcherCommand(command.id);
  }
});

switcherResults.addEventListener("click", event => {
  const result = event.target.closest("[data-switcher-command]");
  if (!result) return;
  executeSwitcherCommand(result.dataset.switcherCommand);
});

switcherLayer.addEventListener("click", event => {
  if (event.target === switcherLayer) closeSwitcher();
});

bellToggle.addEventListener("click", () => {
  if (state.bellOpen) closeBell();
  else openBell();
});

bellLayer.addEventListener("click", event => {
  const route = event.target.closest("[data-hallway-inbox-thread]");
  if (route) {
    routeHallwayInbox(route.dataset.hallwayInboxThread);
    return;
  }
  if (event.target === bellLayer || event.target.closest("[data-close-bell]")) closeBell();
});

bellLayer.addEventListener("keydown", event => {
  if (event.key !== "Tab") return;
  const controls = [...bellLayer.querySelectorAll("button:not([disabled])")];
  if (controls.length === 0) return;
  const first = controls[0];
  const last = controls.at(-1);
  if (event.shiftKey && document.activeElement === first) {
    event.preventDefault();
    last.focus();
  } else if (!event.shiftKey && document.activeElement === last) {
    event.preventDefault();
    first.focus();
  }
});

document.addEventListener("keydown", event => {
  if (state.bellOpen || event.ctrlKey || event.metaKey || event.altKey || event.target.matches("input, textarea, select")) return;
  const view = SUBJECT_VIEWS[Number(event.key) - 1];
  if (!view) return;
  event.preventDefault();
  openSubjectView(view);
});

const MODE_ORDER = ["direct", "hallway", "project"];

document.addEventListener("keydown", event => {
  if (state.bellOpen || !(event.ctrlKey || event.metaKey) || event.altKey || event.target.matches("input, textarea, select")) return;
  if (event.key === "ArrowLeft" || event.key === "ArrowRight") {
    const target = MODE_ORDER[MODE_ORDER.indexOf(state.mode) + (event.key === "ArrowRight" ? 1 : -1)];
    if (!target) return;
    event.preventDefault();
    openMode(target);
    return;
  }
  if (event.key !== "ArrowUp" && event.key !== "ArrowDown") return;
  const rows = visibleConversations();
  const current = rows.findIndex(item => item.id === state.activeId);
  if (current === -1) return;
  const target = rows[current + (event.key === "ArrowDown" ? 1 : -1)];
  if (!target) return;
  event.preventDefault();
  openConversation(target.id);
});

function setInspector(open) {
  shell.dataset.inspectorOpen = String(open);
  inspectorToggle.setAttribute("aria-pressed", String(open));
}

inspectorToggle.addEventListener("click", () => setInspector(shell.dataset.inspectorOpen !== "true"));
document.querySelector(".inspector-close").addEventListener("click", () => setInspector(false));

sidebarToggle.addEventListener("click", () => {
  setMobileSidebarOpen(shell.dataset.sidebarOpen !== "true");
});

sidebarDrawer.addEventListener("click", event => {
  const mechanicsDoor = event.target.closest("[data-open-house-mechanics]");
  if (mechanicsDoor) {
    openHouseMechanics();
    return;
  }
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
    const stateKey = SETTING_STATE_KEYS[setting];
    if (!stateKey) return;
    state[stateKey] = value;
    if (setting === "status-visible") statusPopover.hidden = true;
    render();
  });
});

document.addEventListener("keydown", event => {
  if (event.key !== "Escape") return;
  if (state.bellOpen) {
    event.preventDefault();
    closeBell();
    return;
  }
  if (state.switcherOpen) {
    event.preventDefault();
    closeSwitcher();
    return;
  }
  if (state.sessionMenuOpen) {
    event.preventDefault();
    setSessionMenu(false);
    return;
  }
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
    return;
  }
  if (state.activeId === "house") {
    event.preventDefault();
    leaveHouse();
    return;
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
  if (event.matches) {
    setInspector(false);
    setMobileSidebarOpen(false);
  }
  syncMobileSidebarAccessibility();
});
setDrawerView("root", null, false);
updateComposerState();


render();

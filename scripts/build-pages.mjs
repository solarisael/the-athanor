import { createHash } from "node:crypto";
import { cp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const sourceSite = join(root, "site");
const sourceApp = join(root, "gui-prototype");
const output = join(root, "dist", "pages");
const outputApp = join(output, "app");

const substitutions = [
  [/\bSolarisael House\b/g, "Lumen House"],
  [/\bSolarisael\b/g, "Lumen"],
  [/\bsolarisael\b/g, "lumen"],
  [/\bSolzinho\b/g, "Alex"],
  [/\bSol\b/g, "Alex"],
  [/\bsolzinho\b/g, "alex"],
  [/\bsol\b/g, "alex"],
  [/\bKintsu\b/g, "Aster"],
  [/\bkintsu\b/g, "aster"],
  [/\bKodo\b/g, "Morrow"],
  [/\bkodo\b/g, "morrow"],
  [/\bTuner\b/g, "Vale"],
  [/\btuner\b/g, "vale"],
  [/\bMultistock\b/g, "Harbor Ledger"],
  [/\bmultistock\b/g, "harborLedger"],
  [/\bFamily Hallway\b/g, "Studio Hallway"],
  [/\bSCV\b/g, "review"],
  [/\bSGD\b/g, "delivery"],
  [/\bGPT-5\.6\b/g, "Reasoning model"],
  [/\bClaude Opus\b/g, "Long-context model"],
  [/\buwu\b/gi, ""],
  [/\bòwó\b/gi, ""],
  [/\bövö\b/gi, ""]
];

function assetRevision(content) {
  return createHash("sha256").update(content).digest("hex").slice(0, 12);
}

const publicMessages = [
  "Morning. The interface held its shape overnight.",
  "I checked the measured state, not only the screenshot.",
  "Good. Keep the visible boundary exact.",
  "The collection, instrument, and inspector still agree.",
  "Open the next route and keep the current subject selected.",
  "Receipt recorded on this browser-only specimen."
];

function substitutePublicNames(source) {
  return substitutions.reduce((text, [pattern, replacement]) => text.replace(pattern, replacement), source);
}

function sanitizeFixtureScript(source) {
  let messageIndex = 0;
  let sanitized = substitutePublicNames(source);

  sanitized = sanitized.replace(/text: "(?:[^"\\]|\\.)*"/g, () => {
    const text = publicMessages[messageIndex % publicMessages.length];
    messageIndex += 1;
    return `text: "${text}"`;
  });

  sanitized = sanitized
    .replace(/preview: "(?:[^"\\]|\\.)*"/g, "preview: \"A new addressed message is waiting.\"")
    .replace(/listPreview: "(?:[^"\\]|\\.)*"/g, "listPreview: \"Ready for the next measured pass.\"")
    .replace(/glyph: "K"/g, "glyph: \"A\"")
    .replace(/glyph: "D"/g, "glyph: \"M\"")
    .replace(/glyph: "T"/g, "glyph: \"V\"")
    .replace(/glyph: "S"/g, "glyph: \"A\"")
    .replace(
      "// a real recall replayed as specimen: the 2026-08-17 20:12 turn, values verbatim from its receipt",
      "// fictional public recall fixture; no durable record is loaded"
    )
    .replace("The cardboard Athanor learned its rooms with Alex and Vale", "The interaction shell learned its room boundaries")
    .replace("Alex kept Vale in the live chair until the cardboard Athanor held", "Rendered proof held across every central view")
    .replace("Alex screenshot cut through our controlled proof", "A screenshot exposed a false-positive proof")
    .replace("The day Alex and Aster gave The Athanor its future anatomy", "The day the three-layer interface anatomy settled")
    .replace("Morrow Memory - 2026-05-12 (afternoon → evening)", "Room continuity specimen")
    .replace("session-morrow-hearth-01", "session-morrow-main-01");

  return sanitized;
}

function addPublicDisclosure(source) {
  const disclosure = `  <aside class="public-demo-disclosure" role="note" aria-label="Public specimen disclosure">
    <strong>Public interaction specimen.</strong>
    <span>Fictional fixture records; no Host, database, delivery, or persistence.</span>
    <a href="../">About this demo</a>
  </aside>`;

  return source
    .replace("<title>The Athanor</title>", "<title>The Athanor — Public Interface Specimen</title>")
    .replace("</head>", "  <meta name=\"robots\" content=\"noindex\">\n</head>")
    .replace("<body>", `<body class="public-demo">\n${disclosure}`);
}

const publicDemoCss = `

/* Public Pages wrapper: disclosure sits outside the product anatomy. */
body.public-demo {
  display: grid;
  grid-template-rows: auto minmax(0, 1fr);

  height: 100dvh;
  min-height: 0;

  overflow: hidden;
}

.public-demo-disclosure {
  display: flex;
  align-items: center;
  justify-content: center;

  min-height: 32px;
  gap: 8px;
  padding: 6px 16px;

  font-size: 11px;
  line-height: 1.35;

  border-bottom: 1px solid var(--bd-section);
  background: var(--bg-status);
  color: var(--fg-soft);
}

.public-demo-disclosure strong,
.public-demo-disclosure a {
  color: var(--fg-main);
}

.public-demo-disclosure a:focus-visible {
  outline: 1px solid var(--bd-focus);
  outline-offset: 3px;
}

.public-demo .app-shell {
  width: 100%;
  height: 100%;
  min-height: 0;
}

@media (max-width: 700px) {
  .public-demo-disclosure {
    justify-content: flex-start;
    flex-wrap: wrap;

    min-height: 38px;
    padding: 6px 12px;

    font-size: 10px;
  }
}
`;

await rm(output, { recursive: true, force: true });
await mkdir(outputApp, { recursive: true });
await cp(sourceSite, output, { recursive: true });

for (const file of ["index.html", "colors.css", "styles.css", "app.js"]) {
  await cp(join(sourceApp, file), join(outputApp, file));
}

const siteIndexPath = join(output, "index.html");
const siteStylesPath = join(output, "site.css");
const appIndexPath = join(outputApp, "index.html");
const appColorsPath = join(outputApp, "colors.css");
const appScriptPath = join(outputApp, "app.js");
const appStylesPath = join(outputApp, "styles.css");

const siteStyles = await readFile(siteStylesPath, "utf8");
const appColors = substitutePublicNames(await readFile(appColorsPath, "utf8"));
const appScript = sanitizeFixtureScript(await readFile(appScriptPath, "utf8"));
const appStyles = substitutePublicNames(await readFile(appStylesPath, "utf8")) + publicDemoCss;

const siteIndex = (await readFile(siteIndexPath, "utf8"))
  .replace("href=\"site.css\"", `href=\"site.css?v=${assetRevision(siteStyles)}\"`);
const appIndex = addPublicDisclosure(substitutePublicNames(await readFile(appIndexPath, "utf8")))
  .replace("href=\"colors.css\"", `href=\"colors.css?v=${assetRevision(appColors)}\"`)
  .replace("href=\"styles.css\"", `href=\"styles.css?v=${assetRevision(appStyles)}\"`)
  .replace("src=\"app.js\"", `src=\"app.js?v=${assetRevision(appScript)}\"`)
  .replace(/<span class="avatar avatar-user" aria-hidden="true">S<\/span>/g, "<span class=\"avatar avatar-user\" aria-hidden=\"true\">A</span>")
  .replace(/<span class="account-avatar" aria-hidden="true">S<\/span>/g, "<span class=\"account-avatar\" aria-hidden=\"true\">A</span>")
  .replace(/<span class="settings-avatar" aria-hidden="true">S<\/span>/g, "<span class=\"settings-avatar\" aria-hidden=\"true\">A</span>");

await writeFile(siteIndexPath, siteIndex);
await writeFile(appIndexPath, appIndex);
await writeFile(appColorsPath, appColors);
await writeFile(appScriptPath, appScript);
await writeFile(appStylesPath, appStyles);
await writeFile(join(output, ".nojekyll"), "");

const banned = /\b(?:Sol|Solzinho|Solarisael|Kintsu|Kodo|Tuner|Multistock|SCV|SGD|uwu|òwó|övö)\b/i;
for (const [name, body] of [["app/index.html", appIndex], ["app/app.js", appScript], ["app/styles.css", appStyles]]) {
  const match = body.match(banned);
  if (match) throw new Error(`${name} still contains a private fixture token: ${match[0]}`);
}

console.log(`Built public Pages artifact at ${output}`);

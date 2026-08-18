import { createHash } from "node:crypto";
import { cp, mkdir, readFile, readdir, rm, writeFile } from "node:fs/promises";
import { basename, dirname, isAbsolute, join, relative, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";
import { Marked } from "marked";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const sourceSite = join(root, "site");
const sourceApp = join(root, "gui-prototype");
const output = join(root, "dist", "pages");
const outputApp = join(output, "app");
const sourceDocs = join(root, "docs");
const outputDocs = join(output, "docs");
const rootDocumentationFiles = [
  "README.md",
  "INSTALL.md",
  "USAGE.md",
  "IDENTITY_GUIDE.md",
  "CHANGELOG.md",
  "HOUSE.md",
  "LESSON_MAP.md"
];

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

function fictionalizeRegistryFixtures(source) {
  const start = source.indexOf("const houseSurface = {");
  const end = source.indexOf("\nconst hallwayRecords = {", start);
  if (start === -1 || end === -1) {
    throw new Error("Public fixture registry boundary was not found");
  }

  const fixtures = `const houseSurface = {
  memories: [
    { id: "F-101", date: "2026-08-17 10:06", title: "The interface kept one selected subject", scope: "House fixture", detail: "Fictional record for selection and ownership behavior." },
    { id: "F-102", date: "2026-08-16 21:12", title: "Room memory stayed inside its room", scope: "House fixture", detail: "Fictional record for bounded continuity." },
    { id: "F-103", date: "2026-08-15 18:40", title: "A shared gathering preserved separate identities", scope: "House fixture", detail: "Fictional record for Hallway anatomy." }
  ],
  lessons: [
    { id: "L-201", date: "2026-08-13 16:20", title: "A surface names only the selected subject", kind: "Project fixture" },
    { id: "L-202", date: "2026-08-14 17:32", title: "Project conversations stay with their project", kind: "Project fixture" },
    { id: "L-203", date: "2026-08-14 19:48", title: "Independent effects keep independent lifecycles", kind: "Project fixture" },
    { id: "L-204", date: "2026-08-15 20:15", title: "A local specimen refuses stale assets", kind: "Project fixture" },
    { id: "L-205", date: "2026-08-16 15:03", title: "Dense controls still need breathing room", kind: "Coding fixture" },
    { id: "L-206", date: "2026-08-16 22:41", title: "Session provenance is not navigation", kind: "Project fixture" },
    { id: "L-207", date: "2026-08-17 09:22", title: "Refusals and receipts own their boundaries", kind: "Project fixture" }
  ]
};

const roomMemoryShelves = {
  kintsu: [
    { id: "F-301", date: "2026-08-16 23:10", title: "The room kept its own working continuity", detail: "Fictional room record for this public specimen." },
    { id: "F-302", date: "2026-05-28 20:40", title: "A distinct room opened", detail: "Fictional room record for identity boundaries." }
  ],
  kodo: [],
  tuner: [
    { id: "F-303", date: "2026-08-16 19:05", title: "Rendered proof held across every central view", detail: "Fictional room record with no durable source." },
    { id: "F-304", date: "2026-08-16 20:31", title: "A screenshot exposed an occluded edge", detail: "Fictional room record with no durable source." }
  ]
};
`;

  return source.slice(0, start) + fixtures + source.slice(end);
}


function sanitizeFixtureScript(source) {
  let messageIndex = 0;
  let sanitized = substitutePublicNames(fictionalizeRegistryFixtures(source));

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

async function findMarkdownFiles(directory) {
  const entries = await readdir(directory, { withFileTypes: true });
  const files = [];

  for (const entry of entries) {
    const entryPath = join(directory, entry.name);
    if (entry.isDirectory()) {
      files.push(...await findMarkdownFiles(entryPath));
    } else if (entry.isFile() && entry.name.toLowerCase().endsWith(".md")) {
      files.push(entryPath);
    }
  }

  return files;
}

function routeSegment(value) {
  return value
    .replace(/\.md$/i, "")
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
}

function documentationOutputPath(source) {
  const docsRelativePath = relative(sourceDocs, source);
  const isDocumentationSource = docsRelativePath !== ".."
    && !docsRelativePath.startsWith(`..${sep}`)
    && !isAbsolute(docsRelativePath);

  if (isDocumentationSource) {
    const segments = docsRelativePath.split(sep);
    const file = segments.pop();
    const stem = file.replace(/\.md$/i, "");
    const routeSegments = segments.map(routeSegment);
    if (stem.toLowerCase() !== "readme") routeSegments.push(routeSegment(stem));
    return join(outputDocs, ...routeSegments, "index.html");
  }

  const stem = basename(source).replace(/\.md$/i, "");
  const route = stem.toLowerCase() === "readme" ? "project" : routeSegment(stem);
  return join(outputDocs, route, "index.html");
}

function relativeFileHref(fromFile, toFile) {
  let href = relative(dirname(fromFile), toFile).split(sep).join("/");
  if (!href.startsWith(".")) href = `./${href}`;
  return href;
}

function relativeRouteHref(fromFile, toRoute) {
  if (fromFile === toRoute) return "./";
  let href = relative(dirname(fromFile), dirname(toRoute)).split(sep).join("/");
  if (!href) return "./";
  if (!href.startsWith(".")) href = `./${href}`;
  return `${href}/`;
}

function rewriteMarkdownLinks(markdown, source, target, routes) {
  return markdown.replace(/(!?\[[^\]]*\]\()([^)]+)(\))/g, (match, prefix, rawDestination, suffix) => {
    const destinationMatch = rawDestination.trim().match(/^<?([^\s>]+)>?(\s+.*)?$/s);
    if (!destinationMatch) return match;

    const destination = destinationMatch[1];
    const title = destinationMatch[2] ?? "";
    const hashAt = destination.indexOf("#");
    const pathPart = hashAt === -1 ? destination : destination.slice(0, hashAt);
    const hash = hashAt === -1 ? "" : destination.slice(hashAt);
    if (!pathPart.toLowerCase().endsWith(".md") && !pathPart.endsWith("/")) return match;

    const resolvedSource = resolve(dirname(source), decodeURIComponent(pathPart));
    const resolvedTarget = routes.get(resolvedSource);
    if (!resolvedTarget) return match;

    const href = resolvedTarget === target
      ? hash || "./"
      : `${relativeRouteHref(target, resolvedTarget)}${hash}`;
    return `${prefix}${href}${title}${suffix}`;
  });
}

function escapeHtml(value) {
  return String(value).replace(/[&<>"']/g, character => ({
    "&": "&amp;",
    "<": "&lt;",
    ">": "&gt;",
    "\"": "&quot;",
    "'": "&#39;"
  })[character]);
}

function decodeHtml(value) {
  return value
    .replace(/&#x([0-9a-f]+);/gi, (_, code) => String.fromCodePoint(Number.parseInt(code, 16)))
    .replace(/&#(\d+);/g, (_, code) => String.fromCodePoint(Number.parseInt(code, 10)))
    .replace(/&quot;/g, "\"")
    .replace(/&#39;/g, "'")
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">")
    .replace(/&amp;/g, "&");
}

function headingSlug(value) {
  return decodeHtml(value.replace(/<[^>]+>/g, ""))
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "") || "section";
}

function addHeadingIds(html) {
  const seen = new Map();
  return html.replace(/<h([1-6])>([\s\S]*?)<\/h\1>/g, (heading, level, body) => {
    const base = headingSlug(body);
    const count = seen.get(base) ?? 0;
    seen.set(base, count + 1);
    const id = count === 0 ? base : `${base}-${count + 1}`;
    return `<h${level} id="${id}">${body}</h${level}>`;
  });
}

function replaceMermaidBlocks(html, title, target, diagramAssets) {
  return html.replace(/<pre><code class="language-mermaid">([\s\S]*?)<\/code><\/pre>(?:\s*<p class="diagram-caption">([\s\S]*?)<\/p>)?/g, (_, encodedSource, caption) => {
    const source = `${decodeHtml(encodedSource).trim()}\n`;
    const digest = assetRevision(source);
    const file = `${digest}.svg`;
    if (!diagramAssets.has(file)) {
      throw new Error(`Missing committed Mermaid SVG ${file} for ${title}`);
    }

    const imagePath = join(outputDocs, "diagrams", file);
    const imageHref = relativeFileHref(target, imagePath);
    return `<figure class="doc-diagram">
  <img src="${imageHref}" alt="Architecture flow from ${escapeHtml(title)}">
  ${caption ? `<figcaption>${caption}</figcaption>` : ""}
  <details>
    <summary>Read the diagram source</summary>
    <pre><code>${escapeHtml(source.trim())}</code></pre>
  </details>
</figure>`;
  });
}

function markdownTitle(markdown, source) {
  const match = markdown.match(/^#\s+(.+)$/m);
  return match ? match[1].replace(/[*_`]/g, "").trim() : basename(source).replace(/\.md$/i, "");
}

function documentationPage(title, body, target, routes, stylesRevision) {
  const docsIndex = routes.get(resolve(sourceDocs, "README.md"));
  const homeHref = relativeRouteHref(target, join(output, "index.html"));
  const appHref = relativeRouteHref(target, join(outputApp, "index.html"));
  const docsHref = relativeRouteHref(target, docsIndex);
  const stylesHref = `${relativeFileHref(target, join(output, "site.css"))}?v=${stylesRevision}`;
  const fontHref = relativeFileHref(target, join(output, "fonts", "InterVariable.woff2"));

  return `<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <meta name="description" content="${escapeHtml(title)} — The Athanor documentation.">
  <title>${escapeHtml(title)} — The Athanor</title>
  <link rel="preload" href="${fontHref}" as="font" type="font/woff2" crossorigin>
  <link rel="stylesheet" href="${stylesHref}">
</head>
<body class="documentation-page">
  <header class="site-header">
    <a class="wordmark" href="${homeHref}" aria-label="The Athanor home">
      <span aria-hidden="true">✦</span>
      <strong>The Athanor</strong>
    </a>
    <nav aria-label="Public links">
      <a href="${appHref}">Interface</a>
      <a aria-current="page" href="${docsHref}">Documentation</a>
      <a href="https://github.com/solarisael/the-athanor">Source</a>
    </nav>
  </header>
  <main class="documentation-shell">
    <article class="documentation-content">
${body}
    </article>
  </main>
  <footer>
    <strong>The Athanor</strong>
    <span>Apache-2.0 · Public source · Documentation</span>
  </footer>
</body>
</html>
`;
}

async function buildDocumentation(siteStyles) {
  const documentationSources = [
    ...rootDocumentationFiles.map(file => join(root, file)),
    ...await findMarkdownFiles(sourceDocs)
  ];
  const routes = new Map(documentationSources.map(source => [
    resolve(source),
    documentationOutputPath(source)
  ]));
  const historySource = resolve(sourceDocs, "history");
  const historyTarget = join(outputDocs, "history", "index.html");
  routes.set(historySource, historyTarget);
  const diagramAssets = new Set(await readdir(join(sourceSite, "docs", "diagrams")));
  const parser = new Marked({ gfm: true });
  const stylesRevision = assetRevision(siteStyles);
  const titles = new Map();

  for (const source of documentationSources) {
    const target = routes.get(resolve(source));
    const markdown = await readFile(source, "utf8");
    const title = markdownTitle(markdown, source);
    titles.set(resolve(source), title);
    const linkedMarkdown = rewriteMarkdownLinks(markdown, source, target, routes);
    let body = await parser.parse(linkedMarkdown);
    body = replaceMermaidBlocks(body, title, target, diagramAssets);
    body = addHeadingIds(body);

    await mkdir(dirname(target), { recursive: true });
    await writeFile(target, documentationPage(title, body, target, routes, stylesRevision));
  }

  for (const file of ["LICENSE", "NOTICE"]) {
    await cp(join(root, file), join(outputDocs, "project", file));
  }

  const historySources = documentationSources.filter(source => {
    const historyRelativePath = relative(historySource, source);
    return historyRelativePath !== ".."
      && !historyRelativePath.startsWith(`..${sep}`)
      && !isAbsolute(historyRelativePath);
  });
  const historyItems = historySources
    .sort((left, right) => basename(right).localeCompare(basename(left)))
    .map(source => {
      const route = routes.get(resolve(source));
      return `<li><a href="${relativeRouteHref(historyTarget, route)}">${escapeHtml(titles.get(resolve(source)))}</a></li>`;
    })
    .join("\n");
  const historyBody = addHeadingIds(`<h1>Documentation history</h1>
<p>These dated snapshots preserve how The Athanor changed. They are provenance, not current contracts.</p>
<ul>
${historyItems}
</ul>`);
  await mkdir(dirname(historyTarget), { recursive: true });
  await writeFile(
    historyTarget,
    documentationPage("Documentation history", historyBody, historyTarget, routes, stylesRevision)
  );
}

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
const appStyles = (substitutePublicNames(await readFile(appStylesPath, "utf8")) + publicDemoCss)
  .replaceAll('url("../site/fonts/', 'url("../fonts/');

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
await buildDocumentation(siteStyles);
await writeFile(join(output, ".nojekyll"), "");

const banned = /\b(?:Sol|Solzinho|Solarisael|Kintsu|Kodo|Tuner|Multistock|SCV|SGD|uwu|òwó|övö)\b/i;
for (const [name, body] of [["app/index.html", appIndex], ["app/app.js", appScript], ["app/styles.css", appStyles]]) {
  const match = body.match(banned);
  if (match) throw new Error(`${name} still contains a private fixture token: ${match[0]}`);
}

console.log(`Built public Pages artifact at ${output}`);

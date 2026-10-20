import { createHash } from "node:crypto";
import { cp, lstat, mkdir, readFile, readdir, rm, writeFile } from "node:fs/promises";
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
  "HOUSE.md"
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


function substitutePublicNames(source) {
  return substitutions.reduce((text, [pattern, replacement]) => text.replace(pattern, replacement), source);
}


// This is a reviewed browser boundary, not a recursive copy of the checkout.
// New modules need an explicit privacy review before they enter this list.
const appFiles = [
  "index.html", "colors.css", "styles.css", "app.js",
  "text.js", "repair.js", "markdown.js", "mechanics-live.js", "chat.js",
  "hallways.js", "projects.js", "pulse.js", "health.js", "mechanics.js",
  "board/index.js", "board/hallway-messages.js", "sediment/index.js",
  "sessions/index.js", "preferences/index.js", "continuity/index.js"
];
const banned = /\b(?:Sol|Solzinho|Solarisael|Kintsu|Kodo|Tuner|Multistock|Salvia|Hugo|SCV|SGD|uwu|òwó|övö)\b/i;
const privateConfiguration = /(?:\b[a-z]:[\\/]|\/(?:home|Users)\/|\\\\[a-z0-9._-]+\\|postgres(?:ql)?:\/\/|-----BEGIN [^-]*PRIVATE KEY-----|sk-[a-zA-Z0-9]{20,}|Bearer\s+[a-zA-Z0-9._-]{16,})/i;


function publicScript(name, source) {
  let script = substitutePublicNames(source);
  if (name === "app.js") {
    script = script.replace(
      "This local surface reads Host records and sends operator chat and confirmed session controls. It offers spirit requests and confirmed operator-authored memories and boats when the Host supports them. Lessons remain read-only.",
      "Public disconnected interface. Interface preferences stay in this browser. No Host, model calls, delivery, or durable House records."
    );
  }
  // Keep the existing refusal paths, but never contact a Host from public Pages.
  if (/\bfetch\s*\(/.test(script)) {
    script = `const fetch = async () => {
  throw new Error("Unavailable in the public specimen: no Host connection.");
};
\n${script}`;
  }
  return script;
}

function checkPublicContent(name, body) {
  const match = body.match(banned) ?? body.match(privateConfiguration);
  if (match) throw new Error(`${name} still contains private content`);
}

async function checkAppAssets(files) {
  for (const [name, body] of files) {
    const references = [];
    if (name.endsWith(".js")) {
      if (/\bimport\s*\(/.test(body)) {
        throw new Error(`${name}: dynamic imports require public asset review`);
      }
      for (const match of body.matchAll(/\b(?:import|export)\s+(?:[^;]*?\s+from\s*)?["']([^"']+)["']/g)) {
        if (!match[1].startsWith(".")) throw new Error(`${name}: non-local module ${match[1]}`);
        references.push(match[1]);
      }
      for (const match of body.matchAll(/new URL\(\s*["']([^"']+)["']\s*,\s*import\.meta\.url\s*\)/g)) references.push(match[1]);
    }
    for (const match of body.matchAll(/\b(?:src|href)=["']([^"']+)["']/g)) references.push(match[1]);
    for (const match of body.matchAll(/url\(\s*["']?([^"')\s]+)["']?\s*\)/g)) references.push(match[1]);
    for (const match of body.matchAll(/@import\s+["']([^"']+)["']/g)) references.push(match[1]);

    for (const reference of references) {
      if (reference.startsWith("#")) continue;
      if (/^(?:https?:|mailto:|data:)/i.test(reference)) continue;
      if (reference.includes("${")) throw new Error(`${name}: computed asset requires public review: ${reference}`);
      const path = decodeURIComponent(reference.split(/[?#]/, 1)[0]);
      const target = resolve(dirname(join(outputApp, name)), path);
      const withinOutput = relative(output, target);
      if (isAbsolute(withinOutput) || withinOutput === ".." || withinOutput.startsWith(`..${sep}`)) {
        throw new Error(`${name}: asset leaves public output: ${reference}`);
      }
      const asset = await lstat(target).catch(() => null);
      if (!asset || asset.isSymbolicLink() || (!asset.isFile() && !reference.endsWith("/"))) {
        throw new Error(`${name}: missing public asset: ${reference}`);
      }
    }
  }
}

function addPublicDisclosure(source) {
  const disclosure = `  <aside class="public-demo-disclosure" role="note" aria-label="Public specimen disclosure">
    <strong>Public interaction specimen.</strong>
    <span>No Host, model calls, delivery, or durable House records. Interface preferences stay in this browser.</span>
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
    if (!/\.(?:md|jsonl)$/i.test(pathPart) && !pathPart.endsWith("/")) return match;

    const resolvedSource = resolve(dirname(source), decodeURIComponent(pathPart));
    const resolvedTarget = routes.get(resolvedSource);
    if (!resolvedTarget) return match;

    const href = resolvedTarget === target
      ? hash || "./"
      : `${resolvedTarget.endsWith(".jsonl") ? relativeFileHref(target, resolvedTarget) : relativeRouteHref(target, resolvedTarget)}${hash}`;
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
  const docsIndex = routes.get(resolve(root, "README.md"));
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
  routes.set(resolve(root, "LESSONS.jsonl"), join(output, "LESSONS.jsonl"));
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
  await cp(join(root, "LESSONS.jsonl"), join(output, "LESSONS.jsonl"));

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

const publicApp = new Map();
for (const file of appFiles) {
  const source = join(sourceApp, file);
  const info = await lstat(source);
  if (!info.isFile() || info.isSymbolicLink()) throw new Error(`Unsafe public app source: ${file}`);
  const body = await readFile(source, "utf8");
  publicApp.set(file, file.endsWith(".js") ? publicScript(file, body) : substitutePublicNames(body));
}

const siteIndexPath = join(output, "index.html");
const siteStylesPath = join(output, "site.css");

const siteStyles = await readFile(siteStylesPath, "utf8");
const appColors = publicApp.get("colors.css");
const appScript = publicApp.get("app.js");
const appStyles = (publicApp.get("styles.css") + publicDemoCss)
  .replaceAll('url("../site/fonts/', 'url("../fonts/');

const siteIndex = (await readFile(siteIndexPath, "utf8"))
  .replace("href=\"site.css\"", `href=\"site.css?v=${assetRevision(siteStyles)}\"`);
const appIndex = addPublicDisclosure(publicApp.get("index.html"))
  .replace("href=\"colors.css\"", `href=\"colors.css?v=${assetRevision(appColors)}\"`)
  .replace("href=\"styles.css\"", `href=\"styles.css?v=${assetRevision(appStyles)}\"`)
  .replace("src=\"app.js\"", `src=\"app.js?v=${assetRevision(appScript)}\"`);

await writeFile(siteIndexPath, siteIndex);
publicApp.set("index.html", appIndex);
publicApp.set("styles.css", appStyles);
for (const [name, body] of publicApp) {
  checkPublicContent(name, body);
  const destination = join(outputApp, name);
  await mkdir(dirname(destination), { recursive: true });
  await writeFile(destination, body);
}
await checkAppAssets(publicApp);
await buildDocumentation(siteStyles);
await writeFile(join(output, ".nojekyll"), "");

// No unreviewed file copied from site/ may add another app entry.
const emittedAppFiles = await readdir(outputApp, { recursive: true, withFileTypes: true });
for (const entry of emittedAppFiles) {
  if (entry.isDirectory()) continue;
  const name = relative(outputApp, join(entry.parentPath, entry.name)).split(sep).join("/");
  if (!publicApp.has(name)) throw new Error(`Unreviewed public app file: ${name}`);
}

console.log(`Built public Pages artifact at ${output}`);

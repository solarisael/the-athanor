import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { cp, mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { promisify } from "node:util";
import { test } from "node:test";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const execute = promisify(execFile);

async function fixture(t) {
  const directory = await mkdtemp(join(tmpdir(), "athanor-pages-"));
  t.after(() => rm(directory, { recursive: true, force: true }));
  await mkdir(join(directory, "scripts"));
  await mkdir(join(directory, "docs"));
  await mkdir(join(directory, "node_modules"));
  await cp(join(root, "scripts/build-pages.mjs"), join(directory, "scripts/build-pages.mjs"));
  await cp(join(root, "gui-prototype"), join(directory, "gui-prototype"), { recursive: true });
  await cp(join(root, "site"), join(directory, "site"), { recursive: true });
  await cp(join(root, "node_modules/marked"), join(directory, "node_modules/marked"), { recursive: true });
  await writeFile(join(directory, "package.json"), '{"type":"module"}');
  for (const name of ["README.md", "INSTALL.md", "USAGE.md", "IDENTITY_GUIDE.md", "CHANGELOG.md", "HOUSE.md", "LESSON_MAP.md", "docs/README.md", "LICENSE", "NOTICE"]) {
    await writeFile(join(directory, name), "# Public documentation\n");
  }
  return directory;
}

function build(directory) {
  return execute(process.execPath, [join(directory, "scripts/build-pages.mjs")], { cwd: directory });
}

test("public modules render unavailable telemetry without publishing private records or server files", async t => {
  const directory = await fixture(t);
  await build(directory);
  const app = join(directory, "dist/pages/app");
  const pulse = await import(pathToFileURL(join(app, "pulse.js")));
  const markup = pulse.renderHousePulse();
  assert.match(markup, /Telemetry, lanes, and receipts are unavailable/);
  assert.match(markup, /No Host or private telemetry/);
  assert.doesNotMatch(markup, /95,690,376|4772aa88|a61389335e22/);
  const html = await readFile(join(app, "index.html"), "utf8");
  assert.match(html, /Public interaction specimen/);
  assert.match(html, /no Host, model calls, database, delivery, or persistence/);
  for (const name of ["serve.ts", "live-routes.json", ".scratch/chat-stub.ts", "repair.test.js"]) {
    await assert.rejects(readFile(join(app, name)), { code: "ENOENT" });
  }
});

test("a module dependency absent from the public artifact refuses publication", async t => {
  const directory = await fixture(t);
  const source = join(directory, "gui-prototype/text.js");
  await writeFile(source, `import "./missing.js";\n${await readFile(source, "utf8")}`);
  await assert.rejects(build(directory), /missing public asset: \.\/missing\.js/);
});

test("a missing CSS asset refuses publication", async t => {
  const directory = await fixture(t);
  const source = join(directory, "gui-prototype/styles.css");
  await writeFile(source, `${await readFile(source, "utf8")}\nbody { background-image: url("./missing.png"); }\n`);
  await assert.rejects(build(directory), /missing public asset: \.\/missing\.png/);
});

test("private paths in a leaf module refuse publication", async t => {
  const directory = await fixture(t);
  const source = join(directory, "gui-prototype/text.js");
  await writeFile(source, `${await readFile(source, "utf8")}\n// C:/Users/private-operator/secret\n`);
  await assert.rejects(build(directory), /text\.js still contains private content/);
});

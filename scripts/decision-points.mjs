#!/usr/bin/env bun
// Decision-point budget checker for coding lesson #460.
//
// A decision point is one branch. Cyclomatic complexity of a function is its
// branch count plus one, so a module or crate total is the sum of `CCN - 1`
// across its production functions, which equals the total branch count. The
// ceilings this enforces:
//
//   function  preferred <= 10 branch points, hard 15
//   module    <= 60 decision points
//   crate     warn 50, review 60, refuse above 70
//
// Method, stated because the number is only meaningful with it: block comments,
// line comments, and string literals are stripped first, so prose words like
// "if", "for", and "or" cannot inflate the count. Everything from the first
// `#[cfg(test)]` to end of file is dropped, because tests, generated code, and
// vendored code are excluded from the budget.
//
// This is a text counter, not a semantic analyzer. It exists so the number is
// reproducible and arguable rather than asserted from memory. It reads high on
// flat dispatch (a seven-arm match over seven enum variants counts six) and it
// cannot see a branch expressed as `bool::then`. Both are honest limits: the
// first is real per-variant dispatch, the second is why "reduce the count" must
// mean removing decisions rather than rephrasing them.
//
// Usage:
//   bun scripts/decision-points.mjs                      # the Presence budget
//   bun scripts/decision-points.mjs crates/host crates/akasha
//   bun scripts/decision-points.mjs --functions crates/presence
//
// Exits 1 when any named ceiling is exceeded, so it can gate a change.

import { readdirSync, readFileSync, statSync } from "node:fs";
import path from "node:path";

const FUNCTION_PREFERRED = 10;
const FUNCTION_HARD = 15;
const MODULE_CEILING = 60;
const CRATE_WARN = 50;
const CRATE_REVIEW = 60;
const CRATE_REFUSE = 70;

// The targets this repair was measured against. Passing explicit paths
// overrides them.
const DEFAULT_TARGETS = ["crates/presence", "crates/summoning", "crates/host/src/presence.rs"];

function rustFiles(target) {
  if (!statSync(target).isDirectory()) return target.endsWith(".rs") ? [target] : [];
  return readdirSync(target)
    .flatMap((entry) => rustFiles(path.join(target, entry)))
    .sort();
}

/** Production source only: no tests, no comments, no string bodies. */
export function productionSource(source) {
  const testModule = source.indexOf("#[cfg(test)]");
  let code = testModule === -1 ? source : source.slice(0, testModule);
  code = code.replace(/\/\*[\s\S]*?\*\//g, " ");
  code = code.split("\n").map((line) => line.replace(/\/\/.*$/, "")).join("\n");
  return code.replace(/"(?:[^"\\]|\\.)*"/g, '""');
}

/** Branch points by kind, so a total can be argued with rather than trusted. */
export function branchPoints(code) {
  const arms = (code.match(/=>/g) || []).length - (code.match(/\bmatch\b/g) || []).length;
  return {
    conditional: (code.match(/(^|[^\w.])if\b/gm) || []).length,
    letElse: (code.match(/\blet\b[^;=]*=[^;]*\belse\b/g) || []).length,
    loop: (code.match(/(^|[^\w.])(while|for|loop)\b/gm) || []).length,
    logical: (code.match(/&&|\|\|/g) || []).length,
    matchesMacro: (code.match(/\bmatches!/g) || []).length,
    matchArms: Math.max(arms, 0),
  };
}

function total(counts) {
  return Object.values(counts).reduce((sum, value) => sum + value, 0);
}

function detail(counts) {
  return Object.entries(counts)
    .filter(([, value]) => value)
    .map(([kind, value]) => `${kind}:${value}`)
    .join(" ");
}

/**
 * Per-function branch counts, by brace depth from a `fn` signature.
 *
 * Deliberately crude: it exists to catch a function past the hard ceiling, not
 * to attribute every branch precisely. Nested closures count toward the
 * enclosing function, which is the conservative direction.
 */
function functionBranches(code) {
  const found = [];
  const pattern = /(?:^|\s)fn\s+([A-Za-z_][A-Za-z0-9_]*)/g;
  let match;
  while ((match = pattern.exec(code)) !== null) {
    const open = code.indexOf("{", match.index);
    if (open === -1) continue;
    let depth = 0;
    let end = open;
    for (; end < code.length; end += 1) {
      if (code[end] === "{") depth += 1;
      if (code[end] === "}") {
        depth -= 1;
        if (depth === 0) break;
      }
    }
    found.push({ name: match[1], points: total(branchPoints(code.slice(open, end + 1))) });
  }
  return found;
}

const args = process.argv.slice(2);
const showFunctions = args.includes("--functions");
const targets = args.filter((arg) => !arg.startsWith("--"));
const failures = [];

for (const target of (targets.length ? targets : DEFAULT_TARGETS)) {
  let subtotal = 0;
  console.log(`\n${target}`);
  for (const file of rustFiles(target)) {
    const code = productionSource(readFileSync(file, "utf8"));
    const counts = branchPoints(code);
    const points = total(counts);
    subtotal += points;
    const label = path.relative(target, file) || path.basename(file);
    console.log(`  ${String(points).padStart(4)}  ${label}${detail(counts) ? `   ${detail(counts)}` : ""}`);
    if (points > MODULE_CEILING) {
      failures.push(`module ${file} is ${points} decision points, ceiling ${MODULE_CEILING}`);
    }
    for (const fn of functionBranches(code)) {
      if (showFunctions && fn.points) {
        console.log(`        ${String(fn.points).padStart(2)}  fn ${fn.name}`);
      }
      if (fn.points > FUNCTION_HARD) {
        failures.push(`fn ${fn.name} in ${file} is ${fn.points} branch points, hard ceiling ${FUNCTION_HARD}`);
      } else if (fn.points > FUNCTION_PREFERRED) {
        console.log(`        note: fn ${fn.name} is ${fn.points} branch points; over the preferred ${FUNCTION_PREFERRED}, needs review`);
      }
    }
  }
  const band = subtotal > CRATE_REFUSE
    ? "REFUSE"
    : subtotal > CRATE_REVIEW
      ? "review"
      : subtotal > CRATE_WARN
        ? "warn"
        : "ok";
  console.log(`  ${String(subtotal).padStart(4)}  TOTAL  [${band}]`);
  if (subtotal > CRATE_REFUSE) {
    failures.push(`crate target ${target} is ${subtotal} decision points, refuse above ${CRATE_REFUSE}`);
  }
}

if (failures.length) {
  console.log("\nOver budget:");
  for (const failure of failures) console.log(`  - ${failure}`);
  process.exit(1);
}
console.log("\nEvery target is inside its budget.");

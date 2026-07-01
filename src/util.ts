// Generic helpers: regex/string normalization, JSON IO, fs scaffolding,
// date stamping, latest-user-message lookup.
//
// Independent of plugin-specific concepts (memory, spirit, ledger). If a
// helper takes a "spirit" or "memory" argument, it does not belong here.

import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { RUNTIME_DIR } from "./paths.ts";

export function normalizeForMatch(value) {
  return String(value || "")
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, " ")
    .replace(/\s+/g, " ")
    .trim();
}

export function escapeRegExp(value) {
  return String(value || "").replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

export async function ensureRuntimeDir() {
  await mkdir(RUNTIME_DIR, { recursive: true });
}

export async function readJson(target, fallback = null) {
  try {
    const raw = await readFile(target, "utf8");
    return JSON.parse(raw);
  } catch {
    return fallback;
  }
}

export async function readJsonl(target) {
  try {
    const raw = await readFile(target, "utf8");
    return raw
      .split("\n")
      .map((line) => line.trim())
      .filter(Boolean)
      .map((line) => {
        try {
          return JSON.parse(line);
        } catch {
          return null;
        }
      })
      .filter(Boolean);
  } catch {
    return [];
  }
}

export async function readOptionalText(target) {
  try {
    return String(await readFile(target, "utf8")).replace(/^\uFEFF/, "").trim();
  } catch {
    return "";
  }
}

export async function writeJson(target, value) {
  await ensureRuntimeDir();
  await writeFile(target, `${JSON.stringify(value, null, 2)}\n`, "utf8");
}

export async function writeJsonFile(target, value) {
  await mkdir(path.dirname(target), { recursive: true });
  await writeFile(target, `${JSON.stringify(value, null, 2)}\n`, "utf8");
}

export function localDateStamp(date = new Date()) {
  const year = date.getFullYear();
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const day = String(date.getDate()).padStart(2, "0");
  return `${year}-${month}-${day}`;
}

// Most-recent user message in a chat.messages.transform-shaped messages array.
export function latestUserMessage(messages) {
  const list = Array.isArray(messages) ? messages : [];
  for (let index = list.length - 1; index >= 0; index -= 1) {
    if (list[index]?.info?.role === "user") return list[index];
  }

  return null;
}

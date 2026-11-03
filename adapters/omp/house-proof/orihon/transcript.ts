// Child transcripts: the OMP session files behind named task workers.
// A child writes `<parentStem>/<agentId>.jsonl`; its header carries its own
// session id. The Host stores only {name, session}; this module proves the
// file and pages it on explicit request.
import { closeSync, createReadStream, openSync, readdirSync, readSync, statSync } from "node:fs";
import { createInterface } from "node:readline";
import path from "node:path";
import { imageReferences, message, splitParts, transcriptRecords, type Entry, type ImageReference, type TranscriptRecord } from "./source.ts";

const HEADER_BYTES = 16 * 1024;
const MAX_SIBLINGS = 256;

export function sessionFileStem(file: string): string {
  return file.endsWith(".jsonl") ? file.slice(0, -".jsonl".length) : file;
}

// The session id a transcript header declares, or null when the file is not a session file.
export function sessionHeaderId(file: string): string | null {
  let descriptor: number;
  try {
    descriptor = openSync(file, "r");
  } catch {
    return null;
  }
  try {
    const buffer = Buffer.alloc(HEADER_BYTES);
    const read = readSync(descriptor, buffer, 0, HEADER_BYTES, 0);
    for (const line of buffer.toString("utf8", 0, read).split("\n").slice(0, 4)) {
      try {
        const value = JSON.parse(line);
        if (value?.type === "session" && typeof value.id === "string" && value.id) return value.id;
      } catch {
        // A title slot or a torn line precedes the header.
      }
    }
    return null;
  } finally {
    closeSync(descriptor);
  }
}

// The child session id behind a finished task job, proven by its transcript header.
export function childSessionOf(parentFile: string, name: string): string | null {
  if (!parentFile || !/^[A-Za-z0-9._-]+$/.test(name)) return null;
  return sessionHeaderId(path.join(sessionFileStem(parentFile), `${name}.jsonl`));
}

// The transcript file of a verified child: by name first, then by header among the parent's children.
export function childTranscriptFile(parentFile: string, name: string, session: string): string | null {
  const directory = sessionFileStem(parentFile);
  if (/^[A-Za-z0-9._-]+$/.test(name)) {
    const named = path.join(directory, `${name}.jsonl`);
    if (sessionHeaderId(named) === session) return named;
  }
  let siblings: string[];
  try {
    siblings = readdirSync(directory).filter((file) => file.endsWith(".jsonl")).slice(0, MAX_SIBLINGS);
  } catch {
    return null;
  }
  for (const file of siblings) {
    const candidate = path.join(directory, file);
    if (sessionHeaderId(candidate) === session) return candidate;
  }
  return null;
}

// The top-level session file for a session id, found beside the current session file.
export function sessionFileById(currentFile: string, currentSession: string, session: string): string | null {
  if (session === currentSession) return currentFile;
  const directory = path.dirname(currentFile);
  let files: string[];
  try {
    files = readdirSync(directory).filter((file) => file.endsWith(`_${session}.jsonl`));
  } catch {
    return null;
  }
  const match = files.map((file) => path.join(directory, file)).find((file) => sessionHeaderId(file) === session);
  return match ?? null;
}

export type TranscriptPage = {
  file: string;
  session: string;
  offset: number;
  nextOffset: number | null;
  totalParts: number | null;
  tornTail: boolean;
  records: Array<TranscriptRecord & { part: number; parts: number }>;
};

export const TRANSCRIPT_PAGE_BYTES = 30_000;

// Offsets count deterministic text fragments, not clipped records. A long
// report remains entirely reachable, and paging never loads the whole log.
export async function readTranscriptPage(file: string, session: string, offset: number): Promise<TranscriptPage> {
  if (!Number.isSafeInteger(offset) || offset < 0) throw new Error("transcript offset must be a non-negative safe integer");
  const size = statSync(file).size;
  const input = createReadStream(file, { encoding: "utf8" });
  const lines = createInterface({ input, crlfDelay: Infinity });
  const records: TranscriptPage["records"] = [];
  let consumed = 0;
  let lineNumber = 0;
  let position = 0;
  let bytes = 0;
  let tornTail = false;
  let more = false;
  try {
    for await (const line of lines) {
      lineNumber++;
      consumed += Buffer.byteLength(line, "utf8") + 1;
      if (!line.trim()) continue;
      let decoded: TranscriptRecord[];
      try {
        const entry = JSON.parse(line) as Entry;
        decoded = typeof entry?.id === "string" && typeof entry.type === "string"
          ? transcriptRecords(entry, session) : [];
      } catch {
        if (consumed >= size) {
          tornTail = true;
          break;
        }
        decoded = [{
          sourceId: `line-${lineNumber}:gap`, kind: "echo", speaker: "OMP transcript gap",
          saidAt: "", text: "unparsable transcript line",
        }];
      }
      for (const record of decoded) {
        const parts = splitParts(record.text, TRANSCRIPT_PAGE_BYTES);
        for (let part = 0; part < parts.length; part++) {
          if (position++ < offset) continue;
          const text = parts[part];
          const cost = Buffer.byteLength(text, "utf8");
          if (records.length && bytes + cost > TRANSCRIPT_PAGE_BYTES) {
            more = true;
            break;
          }
          records.push({ ...record, text, part: part + 1, parts: parts.length });
          bytes += cost;
        }
        if (more) break;
      }
      if (more) break;
    }
  } finally {
    lines.close();
    input.destroy();
  }
  return {
    file, session, offset, nextOffset: more ? offset + records.length : null,
    totalParts: more ? null : position, tornTail, records,
  };
}

export async function readRecordedImages(
  file: string, session: string, entryId: string, references: ImageReference[],
): Promise<Array<{ type: "image"; data: string; mimeType: string }>> {
  if (sessionHeaderId(file) !== session) throw new Error("image transcript session does not match recorded provenance");
  if (!references.length || references.length > 8) throw new Error("image reference batch is invalid");
  const input = createReadStream(file, { encoding: "utf8" });
  const lines = createInterface({ input, crlfDelay: Infinity });
  try {
    for await (const line of lines) {
      if (!line.trim()) continue;
      const entry = JSON.parse(line) as Entry;
      if (entry.id !== entryId) continue;
      const content = message(entry)?.content;
      const actual = imageReferences(content);
      return references.map((reference) => {
        const match = actual.find((image) => image.index === reference.index);
        if (!match || match.mimeType !== reference.mimeType || match.sha256 !== reference.sha256) {
          throw new Error("image bytes no longer match recorded provenance");
        }
        const block = content[reference.index];
        return { type: "image", data: block.data, mimeType: block.mimeType };
      });
    }
    throw new Error("image source entry is absent from its recorded transcript");
  } finally {
    lines.close();
    input.destroy();
  }
}

// Store registry for the remember tool (roadmap: memory-write store routing).
// Silhouette: kind -> { script, whenToUse, required, argMap, noBackup }.
// Adding a store later is one registry row, not a new code branch.
//
// The `memory` kind is NOT here — it keeps its dedicated path
// (writeSessionMemory: source-path generation, threads, --body-stdin).
// This registry covers the flat lesson scripts, which all share the shape
// title + lesson-text-on-stdin + optional scalar/tag flags.
//
// `record_cabinet_entry.py` is deliberately absent: it is subcommand-shaped
// (add/append-rep, pillar/cycle semantics) and flattening it into
// title+body would misrepresent its interface. Give it its own row when
// it gets an honest design.

export const REMEMBER_STORES = {
  "coding-lesson": {
    script: "record_coding_lesson.py",
    whenToUse: "a reusable code rule with a proof pattern",
    required: [],
    argMap: {
      shape: "--shape",
      voice: "--voice",
      scope: "--scope",
      project: "--project",
      proofPattern: "--proof-pattern",
      triggerContext: "--trigger-context",
      languageKeys: "--language-key",
      technologyKeys: "--technology-key",
      threadKeys: "--thread-key",
      tags: "--tag",
    },
    noBackup: true,
  },
  "project-lesson": {
    script: "record_project_lesson.py",
    whenToUse: "a project-wide rule (lighter, less personal than a coding lesson)",
    required: ["project"],
    argMap: {
      project: "--project",
      proofPattern: "--proof-pattern",
      triggerContext: "--trigger-context",
      languageKeys: "--language-key",
      technologyKeys: "--technology-key",
      threadKeys: "--thread-key",
      tags: "--tag",
    },
    noBackup: false,
  },
  "writing-lesson": {
    script: "record_writing_lesson.py",
    whenToUse: "a prose-taste rule: register, voice, opening/closing, wit mechanics",
    required: [],
    argMap: {
      voice: "--voice",
      register: "--register",
      shape: "--shape",
      triggerContext: "--trigger-context",
      tags: "--tag",
      threadKeys: "--thread-key",
    },
    noBackup: true,
  },
  "design-lesson": {
    script: "record_design_lesson.py",
    whenToUse: "a design-system taste rule: tokens, component contracts, layout, accessibility",
    required: ["title", "lesson"],
    argMap: {
      voice: "--voice",
      register: "--register",
      shape: "--shape",
      proofPattern: "--proof-pattern",
      triggerContext: "--trigger-context",
      exampleText: "--example-text",
      tags: "--tag",
      threadKeys: "--thread-key",
    },
    noBackup: true,
  },
  "audio-lesson": {
    script: "record_audio_lesson.py",
    whenToUse: "an audio-pipeline rule (tools, stages, commands)",
    required: [],
    argMap: {
      shape: "--shape",
      triggerContext: "--trigger-context",
      tags: "--tag",
      threadKeys: "--thread-key",
    },
    noBackup: false,
  },
};

// Build the kind-specific argv tail from tool params, refusing loudly:
// a missing required field or a field the kind does not accept is an
// error that names the accepted set, never a silent drop.
export function buildStoreArgs(kind, store, fields, requiredValues = {}) {
  const provided = Object.entries(fields).filter(([, value]) =>
    Array.isArray(value) ? value.length > 0 : value !== undefined && value !== null && value !== "",
  );

  for (const name of store.required) {
    const value = Object.prototype.hasOwnProperty.call(requiredValues, name) ? requiredValues[name] : fields[name];
    const present = Array.isArray(value) ? value.length > 0 : value !== undefined && value !== null && value !== "";
    if (!present) {
      return { ok: false, error: `kind '${kind}' requires field '${name}'` };
    }
  }

  const args = [];
  for (const [key, value] of provided) {
    const flag = store.argMap[key];
    if (!flag) {
      const accepted = Object.keys(store.argMap).join(", ") || "(none)";
      return { ok: false, error: `kind '${kind}' does not accept field '${key}'; accepted: ${accepted}` };
    }
    if (Array.isArray(value)) {
      for (const item of value) args.push(flag, String(item));
    } else {
      args.push(flag, String(value));
    }
  }
  return { ok: true, args };
}

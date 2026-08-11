// Typed lesson-family policy for the Rust remember operation.
// PostgreSQL remains authoritative; this registry only validates the stable
// public tool contract and preserves each family's backup requirement.
export const REMEMBER_STORES = {
  "coding-lesson": {
    whenToUse: "a reusable code rule with a proof pattern",
    required: [],
    fields: ["shape", "voice", "scope", "project", "proofPattern", "triggerContext", "languageKeys", "technologyKeys", "threadKeys", "tags", "sourceMemoryPath"],
    backup: false,
  },
  "project-lesson": {
    whenToUse: "a project-wide rule (lighter, less personal than a coding lesson)",
    required: ["project"],
    fields: ["project", "proofPattern", "triggerContext", "languageKeys", "technologyKeys", "threadKeys", "tags", "sourceMemoryPath"],
    backup: true,
  },
  "writing-lesson": {
    whenToUse: "a prose-taste rule: register, voice, opening/closing, wit mechanics",
    required: [],
    fields: ["voice", "register", "shape", "triggerContext", "exampleText", "tags", "threadKeys", "sourceMemoryPath"],
    backup: false,
  },
  "design-lesson": {
    whenToUse: "a design-system taste rule: tokens, component contracts, layout, accessibility",
    required: ["title", "lesson"],
    fields: ["voice", "register", "shape", "proofPattern", "triggerContext", "exampleText", "tags", "threadKeys", "sourceMemoryPath"],
    backup: false,
  },
  "audio-lesson": {
    whenToUse: "an audio-pipeline rule (tools, stages, commands)",
    required: [],
    fields: ["shape", "triggerContext", "tags", "threadKeys", "sourceMemoryPath"],
    backup: true,
  },
};

export function validateStoreFields(kind, store, fields, requiredValues = {}) {
  const provided = Object.entries(fields).filter(([, value]) =>
    Array.isArray(value) ? value.length > 0 : value !== undefined && value !== null && value !== "",
  );
  for (const name of store.required) {
    const value = Object.prototype.hasOwnProperty.call(requiredValues, name) ? requiredValues[name] : fields[name];
    const present = Array.isArray(value) ? value.length > 0 : value !== undefined && value !== null && value !== "";
    if (!present) return { ok: false, error: `kind '${kind}' requires field '${name}'` };
  }
  const invalid = provided.find(([key]) => !store.fields.includes(key));
  if (invalid) {
    return { ok: false, error: `kind '${kind}' does not accept field '${invalid[0]}'; accepted: ${store.fields.join(", ") || "(none)"}` };
  }
  return { ok: true };
}

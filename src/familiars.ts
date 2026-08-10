// The Athanor — familiar registry and dispatch shaping (PURE).
// Silhouette: validate one room spellbook, resolve familiar aliases, and bind a
// familiar to an existing worker lane without reading files or spawning agents.

import {
  buildDispatchReceipt,
  getWorkerLane,
  type ContextHint,
  type DispatchReceipt,
  type RiskLevel,
  type WorkerLaneName,
} from "./routing.ts";

export const FAMILIAR_SPELLBOOK_FILENAMES = ["spellbook.json", "litters.json"] as const;

export type FamiliarDefinition = {
  id: string;
  name: string;
  aliases: string[];
  lane: WorkerLaneName;
  description: string;
  temperament?: string;
  appearance?: string;
};

export type FamiliarSpellbook = {
  version: 1;
  collective: string;
  collectiveAliases: string[];
  spellbookAliases: string[];
  familiars: FamiliarDefinition[];
};

export type FamiliarSpellbookResult = {
  ok: boolean;
  errors: string[];
  spellbook: FamiliarSpellbook | null;
};

export type FamiliarDispatchRequest = {
  familiar: string;
  task: string;
  target?: string;
  context?: ContextHint[];
  acceptance?: string[];
  risk?: RiskLevel;
  lessonBodies?: string[];
};

export type FamiliarDispatchReceipt = DispatchReceipt & {
  familiar: FamiliarDefinition | null;
};

function cleanString(value: unknown): string {
  return typeof value === "string" ? value.trim() : "";
}

function cleanStrings(value: unknown): string[] {
  if (!Array.isArray(value)) return [];
  return [...new Set(value.map(cleanString).filter(Boolean))];
}

function cloneFamiliar(familiar: FamiliarDefinition): FamiliarDefinition {
  return { ...familiar, aliases: [...familiar.aliases] };
}

function familiarTaskName(familiar: FamiliarDefinition): string {
  return familiar.id.replace(/(^|-)([a-z])/g, (_match, _dash, char) => char.toUpperCase()).slice(0, 32);
}

function rejectFamiliarDispatch(errors: string[]): FamiliarDispatchReceipt {
  return {
    ok: false,
    status: "rejected",
    lane: null,
    modelRole: null,
    ompAgent: null,
    familiar: null,
    errors,
    warnings: [],
    dispatcher: {
      executed: false,
      reason: "The familiar request was rejected before a worker packet could be built.",
    },
    spawnPacket: null,
  };
}

function cloneSpellbook(spellbook: FamiliarSpellbook): FamiliarSpellbook {
  return {
    ...spellbook,
    collectiveAliases: [...spellbook.collectiveAliases],
    spellbookAliases: [...spellbook.spellbookAliases],
    familiars: spellbook.familiars.map(cloneFamiliar),
  };
}

export function parseFamiliarSpellbook(value: unknown): FamiliarSpellbookResult {
  const errors: string[] = [];
  const input = value && typeof value === "object" ? value as Record<string, unknown> : null;

  if (!input) {
    return { ok: false, errors: ["Familiar spellbook must be a JSON object."], spellbook: null };
  }

  if (input.version !== 1) errors.push("Familiar spellbook version must be 1.");

  const collective = cleanString(input.collective);
  if (!collective) errors.push("Familiar spellbook collective is required.");

  const collectiveAliases = cleanStrings(input.collectiveAliases);
  const spellbookAliases = cleanStrings(input.spellbookAliases);
  const entries = Array.isArray(input.familiars) ? input.familiars : [];
  if (entries.length === 0) errors.push("Familiar spellbook requires at least one familiar.");

  const familiars: FamiliarDefinition[] = [];
  const lookupKeys = new Map<string, string>();

  entries.forEach((entry, index) => {
    const record = entry && typeof entry === "object" ? entry as Record<string, unknown> : null;
    if (!record) {
      errors.push(`Familiar at index ${index} must be an object.`);
      return;
    }

    const id = cleanString(record.id);
    const name = cleanString(record.name);
    const laneName = cleanString(record.lane);
    const description = cleanString(record.description);
    const aliases = cleanStrings(record.aliases);
    const temperament = cleanString(record.temperament);
    const appearance = cleanString(record.appearance);
    const lane = getWorkerLane(laneName);

    if (!id) errors.push(`Familiar at index ${index} requires an id.`);
    else if (!/^[a-z][a-z0-9-]*$/.test(id)) errors.push(`Familiar id '${id}' must use lowercase kebab-case.`);
    if (!name) errors.push(`Familiar '${id || index}' requires a name.`);
    if (!lane) errors.push(`Familiar '${id || index}' uses unknown worker lane '${laneName || "<empty>"}'.`);
    if (!description) errors.push(`Familiar '${id || index}' requires a description.`);

    const ownerKey = id || String(index);
    const keys = new Set([id, name, ...aliases].filter(Boolean).map((key) => key.toLocaleLowerCase()));
    for (const key of keys) {
      const owner = lookupKeys.get(key);
      if (owner && owner !== ownerKey) errors.push(`Familiar lookup key '${key}' is already owned by '${owner}'.`);
      else lookupKeys.set(key, ownerKey);
    }

    if (id && name && lane && description) {
      familiars.push({
        id,
        name,
        aliases,
        lane: lane.name,
        description,
        ...(temperament ? { temperament } : {}),
        ...(appearance ? { appearance } : {}),
      });
    }
  });

  if (errors.length) return { ok: false, errors, spellbook: null };

  return {
    ok: true,
    errors,
    spellbook: {
      version: 1,
      collective,
      collectiveAliases,
      spellbookAliases,
      familiars,
    },
  };
}

export function listFamiliars(value: unknown): FamiliarSpellbookResult {
  const result = parseFamiliarSpellbook(value);
  return result.spellbook
    ? { ...result, spellbook: cloneSpellbook(result.spellbook) }
    : result;
}

export function getFamiliar(value: unknown, name: string): FamiliarDefinition | null {
  const result = parseFamiliarSpellbook(value);
  if (!result.spellbook) return null;

  const key = cleanString(name).toLocaleLowerCase();
  const familiar = result.spellbook.familiars.find((entry) => (
    entry.id.toLocaleLowerCase() === key
    || entry.name.toLocaleLowerCase() === key
    || entry.aliases.some((alias) => alias.toLocaleLowerCase() === key)
  ));
  return familiar ? cloneFamiliar(familiar) : null;
}

export function buildFamiliarDispatchReceipt(
  value: unknown,
  request: FamiliarDispatchRequest,
): FamiliarDispatchReceipt {
  const parsed = parseFamiliarSpellbook(value);
  if (!parsed.spellbook) return rejectFamiliarDispatch(parsed.errors);

  const familiar = getFamiliar(parsed.spellbook, request?.familiar);
  if (!familiar) {
    return rejectFamiliarDispatch([`Unknown familiar: ${cleanString(request?.familiar) || "<empty>"}`]);
  }

  const dispatch = buildDispatchReceipt({
    lane: familiar.lane,
    task: request.task,
    target: request.target,
    context: request.context,
    acceptance: request.acceptance,
    risk: request.risk,
    lessonBodies: request.lessonBodies,
  });
  const spawnPacket = dispatch.spawnPacket
    ? {
      ...dispatch.spawnPacket,
      args: {
        ...dispatch.spawnPacket.args,
        context: [
          dispatch.spawnPacket.args.context,
          `Familiar: ${familiar.name} (${familiar.id}) — ${familiar.description}`,
          ...(familiar.temperament ? [`Temperament: ${familiar.temperament}`] : []),
          ...(familiar.appearance ? [`Appearance: ${familiar.appearance}`] : []),
        ].join("\n"),
        tasks: [{
          ...dispatch.spawnPacket.args.tasks[0],
          name: familiarTaskName(familiar),
        }] as [typeof dispatch.spawnPacket.args.tasks[0]],
      },
    }
    : null;

  return {
    ...dispatch,
    familiar,
    errors: [...dispatch.errors],
    warnings: [...dispatch.warnings],
    spawnPacket,
  };
}

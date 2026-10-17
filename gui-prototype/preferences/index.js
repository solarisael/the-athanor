const STORAGE_KEY = "athanor.interface.v1";
const CHOICES = {
  density: ["comfortable", "compact"],
  textScale: ["standard", "large"],
  measure: ["focused", "wide"],
  contrast: ["grayscale", "high"]
};
const FLAGS = ["reducedMotion", "timestamps", "sendWithEnter", "statusVisible"];

function preferenceValues(source) {
  if (!source || typeof source !== "object" || Array.isArray(source)) {
    throw new Error("Stored interface preferences are invalid");
  }
  const values = {};
  for (const [key, choices] of Object.entries(CHOICES)) {
    if (choices.includes(source[key])) values[key] = source[key];
  }
  for (const key of FLAGS) {
    if (typeof source[key] === "boolean") values[key] = source[key];
  }
  return values;
}

export function readPreferences() {
  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    return { values: stored === null ? {} : preferenceValues(JSON.parse(stored)), problem: null };
  } catch (error) {
    return { values: {}, problem: `Interface preferences could not be loaded: ${error.message}` };
  }
}

export function writePreferences(state) {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(preferenceValues(state)));
    return null;
  } catch (error) {
    return `Interface preferences could not be saved: ${error.message}`;
  }
}

import { describe, expect, test } from "bun:test";
import { REMEMBER_STORES, validateStoreFields } from "../solarisael-house-proof/stores.ts";

describe("typed Rust remember store policy", () => {
  test("registers every lesson family without Python ownership", () => {
    expect(Object.keys(REMEMBER_STORES)).toEqual([
      "coding-lesson", "project-lesson", "writing-lesson", "design-lesson", "audio-lesson",
    ]);
    for (const [kind, store] of Object.entries(REMEMBER_STORES)) {
      expect(kind).not.toBe("");
      expect(store.whenToUse.trim()).not.toBe("");
      expect(Array.isArray(store.required)).toBe(true);
      expect(Array.isArray(store.fields)).toBe(true);
      expect(typeof store.backup).toBe("boolean");
      expect("script" in store).toBe(false);
    }
  });

  test("preserves family backup requirements", () => {
    expect(REMEMBER_STORES["coding-lesson"].backup).toBe(false);
    expect(REMEMBER_STORES["project-lesson"].backup).toBe(true);
    expect(REMEMBER_STORES["writing-lesson"].backup).toBe(false);
    expect(REMEMBER_STORES["design-lesson"].backup).toBe(false);
    expect(REMEMBER_STORES["audio-lesson"].backup).toBe(true);
  });

  test("accepts complete design fields and rejects cross-family fields", () => {
    expect(validateStoreFields("design-lesson", REMEMBER_STORES["design-lesson"], {
      voice: "system-craft",
      register: ["product-work"],
      shape: "component-contract",
      proofPattern: "Verify keyboard and contrast floors.",
      triggerContext: "Before changing a component.",
      exampleText: "Use the governed token.",
      tags: ["a11y"],
      threadKeys: ["design-contract"],
    }, { title: "Protect the floor", lesson: "Reusable rule." })).toEqual({ ok: true });

    expect(validateStoreFields("design-lesson", REMEMBER_STORES["design-lesson"], { stage: "mix" }, {
      title: "Protect the floor", lesson: "Reusable rule.",
    })).toEqual({
      ok: false,
      error: "kind 'design-lesson' does not accept field 'stage'; accepted: voice, register, shape, proofPattern, triggerContext, exampleText, tags, threadKeys, sourceMemoryPath",
    });
  });

  test("requires project and design identities", () => {
    expect(validateStoreFields("project-lesson", REMEMBER_STORES["project-lesson"], { project: "" })).toEqual({
      ok: false, error: "kind 'project-lesson' requires field 'project'",
    });
    expect(validateStoreFields("design-lesson", REMEMBER_STORES["design-lesson"], {}, {
      title: "Real title", lesson: "",
    })).toEqual({ ok: false, error: "kind 'design-lesson' requires field 'lesson'" });
  });
});

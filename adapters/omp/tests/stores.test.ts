import { describe, expect, test } from "bun:test";

import { REMEMBER_STORES, buildStoreArgs } from "../solarisael-house-proof/stores.ts";

describe("remember store registry", () => {
  test("exposes actionable metadata for every registered store", () => {
    for (const [kind, store] of Object.entries(REMEMBER_STORES)) {
      expect(kind).not.toBe("");
      expect(store.script).toMatch(/\.py$/);
      expect(store.whenToUse.trim()).not.toBe("");
      expect(Array.isArray(store.required)).toBe(true);
      expect(typeof store.argMap).toBe("object");
      expect(store.argMap).not.toBeNull();
      expect(typeof store.noBackup).toBe("boolean");

      for (const required of store.required) {
        if (!["title", "lesson"].includes(required)) {
          expect(Object.keys(store.argMap)).toContain(required);
        }
      }

      for (const [field, flag] of Object.entries(store.argMap)) {
        expect(field).not.toBe("");
        expect(flag).toMatch(/^--[a-z][a-z-]*$/);
      }
    }
  });

  test("registers the design lesson fallback writer", () => {
    expect(REMEMBER_STORES["design-lesson"]).toEqual({
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
    });
  });
});

describe("buildStoreArgs", () => {
  test("builds argv with repeated tag flags for accepted coding lesson fields", () => {
    const result = buildStoreArgs("coding-lesson", REMEMBER_STORES["coding-lesson"], {
      shape: "process",
      voice: "example-craft",
      proofPattern: "pin observable behavior",
      tags: ["routing", "recall"],
    });

    expect(result).toEqual({
      ok: true,
      args: [
        "--shape",
        "process",
        "--voice",
        "example-craft",
        "--proof-pattern",
        "pin observable behavior",
        "--tag",
        "routing",
        "--tag",
        "recall",
      ],
    });
  });

  test("builds repeated register flags for writing lessons", () => {
    const result = buildStoreArgs("writing-lesson", REMEMBER_STORES["writing-lesson"], {
      register: ["fiction", "product-work"],
    });

    expect(result).toEqual({
      ok: true,
      args: ["--register", "fiction", "--register", "product-work"],
    });
  });

  test("builds the design lesson argv with its complete accepted field set", () => {
    const result = buildStoreArgs("design-lesson", REMEMBER_STORES["design-lesson"], {
      voice: "system-craft",
      register: ["product-work", "interface-copy"],
      shape: "component-contract",
      proofPattern: "Verify the contrast and interaction floor.",
      triggerContext: "Before introducing a component variant.",
      exampleText: "Buttons keep one primary action per surface.",
      tags: ["tokens", "a11y"],
      threadKeys: ["design-contract"],
    }, {
      title: "Protect the contrast floor",
      lesson: "Reusable design-system rule.",
    });

    expect(result).toEqual({
      ok: true,
      args: [
        "--voice",
        "system-craft",
        "--register",
        "product-work",
        "--register",
        "interface-copy",
        "--shape",
        "component-contract",
        "--proof-pattern",
        "Verify the contrast and interaction floor.",
        "--trigger-context",
        "Before introducing a component variant.",
        "--example-text",
        "Buttons keep one primary action per surface.",
        "--tag",
        "tokens",
        "--tag",
        "a11y",
        "--thread-key",
        "design-contract",
      ],
    });
  });

  test("requires a title and lesson for design lessons", () => {
    const result = buildStoreArgs("design-lesson", REMEMBER_STORES["design-lesson"], {}, {
      title: "A real title",
      lesson: "",
    });

    expect(result).toEqual({
      ok: false,
      error: "kind 'design-lesson' requires field 'lesson'",
    });
  });

  test("refuses unknown design lesson fields and names the accepted field set", () => {
    const result = buildStoreArgs("design-lesson", REMEMBER_STORES["design-lesson"], {
      stage: "mix",
    }, {
      title: "A real title",
      lesson: "A real lesson.",
    });

    expect(result).toEqual({
      ok: false,
      error: "kind 'design-lesson' does not accept field 'stage'; accepted: voice, register, shape, proofPattern, triggerContext, exampleText, tags, threadKeys",
    });
  });

  test("refuses unknown fields and names the accepted field set", () => {
    const result = buildStoreArgs("project-lesson", REMEMBER_STORES["project-lesson"], {
      project: "solarisael-house",
      alien: "not part of this script interface",
    });

    expect(result).toEqual({
      ok: false,
      error: "kind 'project-lesson' does not accept field 'alien'; accepted: project, proofPattern, triggerContext, languageKeys, technologyKeys, threadKeys, tags",
    });
  });

  test("requires project lessons to name a non-empty project", () => {
    const result = buildStoreArgs("project-lesson", REMEMBER_STORES["project-lesson"], {
      project: "",
      tags: [],
    });

    expect(result).toEqual({
      ok: false,
      error: "kind 'project-lesson' requires field 'project'",
    });
  });

  test("treats empty optional strings and arrays as absent", () => {
    const result = buildStoreArgs("coding-lesson", REMEMBER_STORES["coding-lesson"], {
      shape: "",
      voice: "example-craft",
      tags: [],
      triggerContext: null,
      scope: undefined,
    });

    expect(result).toEqual({
      ok: true,
      args: ["--voice", "example-craft"],
    });
  });
});

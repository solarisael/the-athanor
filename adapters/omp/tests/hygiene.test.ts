import { expect, test } from "bun:test";

import { evaluateAstEdit } from "../hygiene.ts";

test("direct and mounted AST Edit reject the invalid double-dollar metavariable", () => {
  expect(evaluateAstEdit({
    toolName: "ast_edit",
    input: { ops: [{ pat: "$$NAME", out: "value" }] },
  })).toMatchObject({
    block: true,
    reason: "Refusing AST Edit operation 1: $$NAME is invalid. Use $$$NAME for zero-or-more nodes.",
  });

  for (const pat of [
    "console.log(`${call($$ARGS)}`)",
    "call(/* don't */ $$ARGS)",
    "call(/'/, $$ARGS)",
    "console.log(\"$$USD\")",
    "console.log(`$$USD`)",
  ]) {
    expect(evaluateAstEdit({
      toolName: "ast_edit",
      input: { ops: [{ pat, out: "value" }] },
    })).toMatchObject({ block: true });
  }

  expect(evaluateAstEdit({
    toolName: "write",
    input: {
      path: "xd://ast_edit",
      content: JSON.stringify({ ops: [{ pat: "$A", out: "$$VALUE" }] }),
    },
  })).toMatchObject({ block: true });
});

test("single and triple dollar AST captures remain available", () => {
  for (const pat of ["$NAME", "call($$$ARGS)"]) {
    expect(evaluateAstEdit({
      toolName: "ast_edit",
      input: { ops: [{ pat, out: "$NAME" }] },
    })).toBeNull();
  }

  expect(evaluateAstEdit({
    toolName: "write",
    input: { path: "xd://ast_edit", content: "not-json" },
  })).toBeNull();
  expect(evaluateAstEdit({
    toolName: "write",
    input: { path: "owned.ts", content: "$$NAME" },
  })).toBeNull();
});

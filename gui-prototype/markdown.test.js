// The bubble's Markdown is escape-first: wire text never becomes markup, and
// the few shapes it reads come out as the elements the stylesheet expects.
import { test, expect } from "bun:test";
import { renderMarkdown } from "./markdown.js";

test("wire text never becomes markup, even inside code and emphasis", () => {
  expect(renderMarkdown("<script>alert(1)</script>")).toBe("<p>&lt;script&gt;alert(1)&lt;/script&gt;</p>");
  expect(renderMarkdown("`<b>x</b>` and **<i>y</i>**")).toBe("<p><code>&lt;b&gt;x&lt;/b&gt;</code> and <strong>&lt;i&gt;y&lt;/i&gt;</strong></p>");
  expect(renderMarkdown("```\n<div>\n```")).toBe("<pre><code>&lt;div&gt;</code></pre>");
});

test("paragraphs, breaks, lists, headings, and emphasis take their shapes", () => {
  expect(renderMarkdown("one\ntwo\n\nthree")).toBe("<p>one<br>two</p><p>three</p>");
  expect(renderMarkdown("- a\n- b")).toBe("<ul><li>a</li><li>b</li></ul>");
  expect(renderMarkdown("1. a\n2) b")).toBe("<ol><li>a</li><li>b</li></ol>");
  expect(renderMarkdown("## Title")).toBe("<p><strong>Title</strong></p>");
  expect(renderMarkdown("*soft* and _low_ and **hard**")).toBe("<p><em>soft</em> and <em>low</em> and <strong>hard</strong></p>");
});

test("asterisks that are not emphasis stay literal, and code keeps its underscores", () => {
  expect(renderMarkdown("2 * 3 * 4")).toBe("<p>2 * 3 * 4</p>");
  expect(renderMarkdown("`snake_case_name`")).toBe("<p><code>snake_case_name</code></p>");
  expect(renderMarkdown("")).toBe("");
});

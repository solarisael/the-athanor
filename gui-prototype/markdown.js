// The small Markdown the chat bubble reads: paragraphs, line breaks, fenced
// code, inline code, bold, italic, bullet and numbered lists, and headings
// as bold lines. Nothing else — no links, no raw HTML, no images. Every
// character is escaped before any shape is recognized, so text from the wire
// can never become markup.

import { escapeHtml } from "./text.js";

export function renderMarkdown(text) {
  const source = String(text ?? "");
  const blocks = [];
  const fences = source.split(/^```[^\n]*\n?/m);
  fences.forEach((chunk, index) => {
    if (index % 2 === 1) {
      blocks.push(`<pre><code>${escapeHtml(chunk.replace(/\n$/, ""))}</code></pre>`);
    } else {
      for (const paragraph of chunk.split(/\n{2,}/)) {
        const rendered = renderParagraph(paragraph);
        if (rendered) blocks.push(rendered);
      }
    }
  });
  return blocks.join("");
}

function renderParagraph(paragraph) {
  const lines = paragraph.split("\n").filter(line => line.trim() !== "");
  if (lines.length === 0) return "";
  if (lines.every(line => /^\s*[-*]\s+/.test(line))) {
    return `<ul>${lines.map(line => `<li>${inline(line.replace(/^\s*[-*]\s+/, ""))}</li>`).join("")}</ul>`;
  }
  if (lines.every(line => /^\s*\d+[.)]\s+/.test(line))) {
    return `<ol>${lines.map(line => `<li>${inline(line.replace(/^\s*\d+[.)]\s+/, ""))}</li>`).join("")}</ol>`;
  }
  if (lines.length === 1 && /^#{1,6}\s+/.test(lines[0])) {
    return `<p><strong>${inline(lines[0].replace(/^#{1,6}\s+/, ""))}</strong></p>`;
  }
  return `<p>${lines.map(inline).join("<br>")}</p>`;
}

// Inline shapes run on escaped text. Code spans are lifted out first so their
// asterisks and underscores stay literal.
function inline(line) {
  const codes = [];
  const lifted = escapeHtml(line).replace(/`([^`]+)`/g, (_, code) => {
    codes.push(`<code>${code}</code>`);
    return `\u0000${codes.length - 1}\u0000`;
  });
  return lifted
    .replace(/\*\*(.+?)\*\*/g, "<strong>$1</strong>")
    .replace(/(^|[\s(])\*(?!\s)(.+?)(?<!\s)\*(?=[\s.,;:!?)]|$)/g, "$1<em>$2</em>")
    .replace(/(^|[\s(])_(?!\s)(.+?)(?<!\s)_(?=[\s.,;:!?)]|$)/g, "$1<em>$2</em>")
    .replace(/\u0000(\d+)\u0000/g, (_, index) => codes[Number(index)]);
}

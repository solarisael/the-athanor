// Shared text safety for every module that interpolates data into markup.

// The apostrophe is in the set because single-quoted attributes exist in this
// page's markup and in any hand that edits it later; leaving it out makes the
// escape depend on quoting style rather than on the text.
export function escapeHtml(value) {
  return String(value ?? "").replace(/[&<>"']/g, character => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", "\"": "&quot;", "'": "&#39;" })[character]);
}

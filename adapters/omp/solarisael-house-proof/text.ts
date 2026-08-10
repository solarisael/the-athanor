// Text extraction and tiny pure formatting helpers.
// Silhouette: turn OMP's loose message shapes into stable strings.

export function localDateStamp(date = new Date()) {
  const year = date.getFullYear();
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const day = String(date.getDate()).padStart(2, "0");
  return `${year}-${month}-${day}`;
}

export function messageText(message) {
  if (!message) return "";
  if (typeof message === "string") return message;
  if (typeof message.text === "string") return message.text;
  if (typeof message.content === "string") return message.content;
  if (Array.isArray(message.content)) {
    return message.content
      .map((part) => typeof part === "string" ? part : part?.text || part?.content || "")
      .filter(Boolean)
      .join("\n");
  }
  if (Array.isArray(message.parts)) {
    return message.parts
      .map((part) => typeof part === "string" ? part : part?.text || part?.content || "")
      .filter(Boolean)
      .join("\n");
  }
  return "";
}

function textFromContentParts(parts, { assistantTextOnly = false } = {}) {
  if (!Array.isArray(parts)) return "";
  return parts
    .map((part) => {
      if (typeof part === "string") return part;
      if (!part || typeof part !== "object") return "";
      if (assistantTextOnly && part.type !== "text") return "";
      if (typeof part.text === "string") return part.text;
      if (!assistantTextOnly && typeof part.content === "string") return part.content;
      return "";
    })
    .filter(Boolean)
    .join("\n");
}

export function conversationText(message) {
  if (!message || (message.role !== "user" && message.role !== "assistant")) return "";
  if (typeof message.content === "string") return message.content;
  if (Array.isArray(message.content)) {
    return textFromContentParts(message.content, { assistantTextOnly: message.role === "assistant" });
  }
  if (Array.isArray(message.parts)) {
    return textFromContentParts(message.parts, { assistantTextOnly: message.role === "assistant" });
  }
  if (typeof message.text === "string") return message.text;
  return "";
}

export function smallHash(value) {
  let hash = 2166136261;
  for (const ch of String(value || "")) {
    hash ^= ch.charCodeAt(0);
    hash = Math.imul(hash, 16777619);
  }
  return (hash >>> 0).toString(16);
}

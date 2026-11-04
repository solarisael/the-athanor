import { readFile } from "node:fs/promises";

export default function replayRecallExtension(pi: any) {
  pi.on("context", async (event: any) => {
    const messages = Array.isArray(event?.messages) ? event.messages : [];
    if (messages.some((message: any) => message?.role === "custom"
      && message?.customType === "solarisael-recall-context"
      && message?.details?.replay === true)) return;
    const sourcePath = process.env.SOLARISAEL_REPLAY_VIEWPORT;
    if (!sourcePath) return;
    const viewport = JSON.parse(await readFile(sourcePath, "utf8"));
    if (!viewport?.found) return;
    return {
      messages: [
        ...messages,
        {
          role: "custom",
          customType: "solarisael-recall-context",
          content: [
            "<system-reminder>",
            "Room-local Solarisael recall for this user turn.",
            JSON.stringify(viewport, null, 2),
            "</system-reminder>",
          ].join("\n"),
          display: false,
          details: { replay: true, query: viewport.query, found: viewport.found },
          attribution: "agent",
          timestamp: Date.now(),
        },
      ],
    };
  });
}

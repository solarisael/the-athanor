#!/usr/bin/env node
import {
  closeService,
  errorPayload,
  index,
  indexInput,
  search,
  searchInput,
  status,
  statusInput,
} from "./service.ts";

type Request = { operation: "search" | "index" | "status"; input: unknown };

async function main(): Promise<void> {
  try {
    const chunks: Buffer[] = [];
    for await (const chunk of process.stdin) chunks.push(Buffer.from(chunk));
    const request = JSON.parse(Buffer.concat(chunks).toString("utf8")) as Request;
    let data: Record<string, unknown>;

    switch (request.operation) {
      case "search":
        data = await search(searchInput.parse(request.input));
        break;
      case "index":
        data = await index(indexInput.parse(request.input));
        break;
      case "status":
        data = await status(statusInput.parse(request.input));
        break;
      default:
        throw new Error("Unknown workspace search operation.");
    }
    process.stdout.write(JSON.stringify({ ok: true, data }));
  } catch (error) {
    process.stdout.write(JSON.stringify({ ok: false, error: errorPayload(error) }));
  } finally {
    await closeService();
  }
}

await main();

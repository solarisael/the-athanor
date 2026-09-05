import { describe, expect, test } from "bun:test";
import { backupReceiptView, createToolRenderers } from "../house-proof/feedback.ts";

const SHA = "3b4c".repeat(16);

function response(payload: Record<string, unknown>) {
  return { isError: false, content: [{ type: "text" as const, text: JSON.stringify(payload) }], details: payload };
}

function rendered(operation: string, payload: Record<string, unknown>, args: Record<string, unknown>, expanded = false) {
  const renderers = createToolRenderers("Athanor Remember", operation) as {
    renderResult(result: unknown, options: { expanded?: boolean }, theme: undefined, args?: unknown): { render(width: number): string[] };
  };
  return renderers.renderResult(response(payload), { expanded }, undefined, args).render(120).join("\n");
}

describe("backup receipt on durable-write feedback", () => {
  test("an ok backup names the dump, checksum, size, duration, and tool", () => {
    const view = backupReceiptView({
      backup: {
        status: "ok",
        dump_path: "C:\\ProgramData\\Solarisael\\Athanor\\state\\substrate\\backups\\athanor-9f1c.dump",
        sha256: SHA,
        bytes: 5_242_880,
        elapsed_ms: 830,
        tool: "wsl:pg_dump",
      },
    }, undefined);
    expect(view.status).toBe("ok");
    expect(view.attention).toBe(false);
    expect(view.line?.text).toBe("◆ backup ok · athanor-9f1c.dump · sha256 3b4c3b4c3b4c · 5.0 MiB · 830 ms · wsl:pg_dump");
    expect(view.detailLines.map((line) => line.text)).toEqual([
      "dump · C:\\ProgramData\\Solarisael\\Athanor\\state\\substrate\\backups\\athanor-9f1c.dump",
      `sha256 · ${SHA}`,
    ]);
  });

  test("a failed backup carries its mechanical code and one-line detail and demands attention", () => {
    const view = backupReceiptView({
      backup: {
        status: "failed",
        elapsed_ms: 12,
        code: "pg_dump_not_found",
        detail: "pg_dump not found; probed wsl:pg_dump=wsl.exe --exec (program not found), path:pg_dump=pg_dump.exe (program not found)",
      },
    }, undefined);
    expect(view.status).toBe("failed");
    expect(view.attention).toBe(true);
    expect(view.line?.text).toBe(
      "▲ backup failed · pg_dump_not_found · pg_dump not found; probed wsl:pg_dump=wsl.exe --exec (program not found), path:pg_dump=pg_dump.exe (program not found)",
    );
  });

  test("a skipped backup is stated quietly; an absent field renders nothing", () => {
    expect(backupReceiptView({ backup: { status: "skipped" } }, undefined)).toEqual({
      status: "skipped",
      line: { text: "backup skipped", color: "dim" },
      detailLines: [],
      attention: false,
    });
    expect(backupReceiptView({}, undefined).line).toBeNull();
  });

  test("the remember frame shows the backup line and turns its header to a warning on failure", () => {
    const okFrame = rendered("remember", {
      memory_id: 4470,
      room: "kodo",
      source_path: "db-only/kodo/x.md",
      durable: true,
      authority: "postgres",
      backup: { status: "ok", dump_path: "/state/backups/db-1.dump", sha256: SHA, bytes: 10, elapsed_ms: 5, tool: "path:pg_dump" },
      warnings: [],
    }, { title: "afternoon" });
    expect(okFrame).toContain("◆ backup ok · db-1.dump · sha256 3b4c3b4c3b4c · 10 B · 5 ms · path:pg_dump");
    expect(okFrame).toContain("◆ Athanor Remember · Memory #4470");

    const failedFrame = rendered("remember", {
      memory_id: 4471,
      room: "kodo",
      source_path: "db-only/kodo/y.md",
      durable: true,
      authority: "postgres",
      backup: { status: "failed", elapsed_ms: 3, code: "backup_error.command", tool: "wsl:pg_dump", detail: "pg_dump: error: exit status 1" },
      warnings: ["backup failed after PostgreSQL commit (backup_error.command): pg_dump: error: exit status 1"],
    }, { title: "afternoon" });
    expect(failedFrame).toContain("▲ backup failed · backup_error.command · wsl:pg_dump · pg_dump: error: exit status 1");
    expect(failedFrame).toContain("▲ Athanor Remember · Memory #4471");
  });

  test("the sleep frame reads the backup object, not the retired backup_status string", () => {
    const frame = rendered("sleep", {
      ok: true,
      memory_id: "4472",
      room: "kodo",
      source_path: "db-only/paper-boats/z.md",
      outbox_event_id: "evt-1",
      inserted: true,
      durable: true,
      authority: "postgres",
      backup: { status: "ok", dump_path: "/state/backups/db-2.dump", sha256: SHA, bytes: 2048, elapsed_ms: 900, tool: "wsl:pg_dump" },
      warnings: [],
    }, { body: "little next-me" }, true);
    expect(frame).toContain("◆ boat cast · backup ok");
    expect(frame).toContain("◆ backup ok · db-2.dump · sha256 3b4c3b4c3b4c · 2.0 KiB · 900 ms · wsl:pg_dump");
    expect(frame).toContain("continuity ready for the next session");
    expect(frame).toContain(`sha256 · ${SHA}`);

    const failed = rendered("sleep", {
      ok: true,
      memory_id: "4473",
      room: "kodo",
      source_path: "db-only/paper-boats/w.md",
      outbox_event_id: "evt-2",
      inserted: true,
      durable: true,
      authority: "postgres",
      backup: { status: "failed", elapsed_ms: 1, code: "pg_dump_not_found", detail: "pg_dump not found; probed path:pg_dump=pg_dump.exe (program not found)" },
      warnings: [],
    }, { body: "little next-me" });
    expect(failed).toContain("▲ boat cast · backup failed");
    expect(failed).toContain("▲ backup failed · pg_dump_not_found · pg_dump not found; probed path:pg_dump=pg_dump.exe (program not found)");
    expect(failed).not.toContain("continuity ready");
  });
});

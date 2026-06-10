// The single seam for everything that crosses the Windows→WSL door:
// path translation + child-process spawn with timeout.
//
// Extracted 2026-06-10. spawnPostgresSource (memory.ts) and
// runCodingLessonsByShape (triggers.ts) had grown the same spawn machinery
// independently — settle-once latch, kill-timer, buffer accumulation.
// Single-source seam: the machinery lives here once; callers map the raw
// outcome onto their own result contracts.

import { spawn } from "node:child_process";

// Translate `C:\foo\bar` → `/mnt/c/foo/bar` for WSL spawn argv. Falls
// through to the original for non-Windows-absolute paths.
export function windowsPathToWsl(value) {
  const source = String(value || "").replace(/\\/g, "/");
  const match = /^([A-Za-z]):\/(.*)$/.exec(source);
  if (!match) return source;

  return `/mnt/${match[1].toLowerCase()}/${match[2]}`;
}

// Run `wsl.exe <argv>` with a kill-timeout. Never rejects — always resolves
// to a raw outcome the caller maps onto its own contract:
//
//   { timedOut, spawnError, code, stdout, stderr }
//
// stdin (when provided) is piped and closed immediately — long prompts ride
// stdin to dodge argv-length limits. When omitted, stdin still gets closed
// so a child that reads it can't hang the timer.
export function runWsl({ argv, cwd, stdin = null, timeoutMs }) {
  return new Promise((resolve) => {
    const child = spawn("wsl.exe", argv, {
      cwd,
      windowsHide: true,
      stdio: ["pipe", "pipe", "pipe"],
    });

    let stdout = "";
    let stderr = "";
    let settled = false;
    const settle = (value) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      resolve(value);
    };
    const timer = setTimeout(() => {
      child.kill();
      settle({ timedOut: true, spawnError: null, code: null, stdout, stderr });
    }, timeoutMs);

    try {
      child.stdin?.end(stdin === null ? undefined : String(stdin));
    } catch {
      /* fail-soft — a broken stdin pipe shouldn't kill the run */
    }

    child.stdout?.on("data", (chunk) => { stdout += chunk.toString("utf8"); });
    child.stderr?.on("data", (chunk) => { stderr += chunk.toString("utf8"); });
    child.on("error", (err) => settle({
      timedOut: false, spawnError: err?.message || String(err), code: null, stdout, stderr,
    }));
    child.on("close", (code) => settle({
      timedOut: false, spawnError: null, code, stdout, stderr,
    }));
  });
}

import path from "node:path";
import { existsSync, realpathSync } from "node:fs";
import { homedir } from "node:os";

async function configDirectory(executable: string, entrypoint: string | undefined, cwd: string, configFiles: string[]): Promise<string> {
  const child = Bun.spawn([executable, ...(entrypoint ? [entrypoint] : []), "config", "path"], {
    cwd, env: { ...process.env, PI_CONFIG_FILES: configFiles.join(path.delimiter) },
    stdin: "ignore", stdout: "pipe", stderr: "ignore",
  });
  const timer = setTimeout(() => child.kill(), 15_000);
  const output = async () => {
    const reader = child.stdout.getReader();
    const chunks: Uint8Array[] = [];
    let bytes = 0;
    try {
      for (;;) {
        const chunk = await reader.read();
        if (chunk.done) break;
        bytes += chunk.value.byteLength;
        if (bytes > 8_192) throw new Error("OMP config path exceeds its output bound");
        chunks.push(chunk.value);
      }
      return Buffer.concat(chunks).toString("utf8").trim();
    } finally {
      reader.releaseLock();
    }
  };
  try {
    const [directory, code] = await Promise.all([output(), child.exited]);
    if (code !== 0 || !path.isAbsolute(directory) || /[\r\n\0]/.test(directory)) {
      throw new Error("The active OMP runtime could not report its agent directory");
    }
    return realpathSync(directory);
  } finally {
    clearTimeout(timer);
    if (child.exitCode === null) child.kill();
    await child.exited;
  }
}

async function configFiles(pi: any): Promise<string[]> {
  const inherited = process.env.PI_CONFIG_FILES?.split(path.delimiter).filter(Boolean) ?? [];
  const script = process.argv[1];
  const scriptOnDisk = Boolean(script) && path.isAbsolute(script) && existsSync(script);
  const argv = process.argv.slice(scriptOnDisk ? 2 : 1);
  if (!inherited.length && !argv.some((arg) => arg === "--config" || arg.startsWith("--config="))) return [];
  let flags: any;
  try {
    // The same runtime-relative, side-effect-free flag table used by tern-capture.ts.
    const base = scriptOnDisk ? path.dirname(script) : path.dirname(process.execPath);
    flags = await import(Bun.resolveSync("@oh-my-pi/pi-coding-agent/cli/flag-tables", base));
  } catch {
    throw new Error("Hosted folding cannot read this OMP runtime's CLI flag contract; its config selection was not copied");
  }
  const files = [...inherited];
  let launchCwd: string | undefined;
  for (let index = 0; index < argv.length; index++) {
    const arg = argv[index];
    if (arg === "--") break;
    if (!arg.startsWith("-")) continue;
    const equals = arg.startsWith("--") ? arg.indexOf("=") : -1;
    const flag = equals < 0 ? arg : arg.slice(0, equals);
    const inline = equals < 0 ? undefined : arg.slice(equals + 1);
    if ((flag.startsWith("--") && pi.getFlag?.(flag.slice(2)) !== undefined)
      || flags.EXTENSION_SHADOWABLE_STRING_FLAGS.has(flag)) {
      throw new Error("Hosted folding cannot reconstruct extension-shadowed CLI config selection");
    }
    if (flags.STRING_VALUE_FLAGS.has(flag) || flag === "--profile" || flag === "--alias") {
      const value = inline ?? argv[++index];
      if (value === undefined || value === "") throw new Error("Hosted folding refuses an incomplete value-taking CLI flag");
      if (flag === "--config") files.push(value);
      if (flag === "--cwd") launchCwd = value;
    } else if (flags.OPTIONAL_VALUE_FLAGS.has(flag)) {
      const next = argv[index + 1];
      if (inline === undefined && next !== undefined && !next.startsWith("-")
        && !(flags.OPTIONAL_FLAGS[flag].rejectEmpty && next.length === 0)) index++;
    } else if (!flags.VALUELESS_FLAGS.has(flag) && !["-h", "-v", "-c", "-p"].includes(flag)) {
      throw new Error("Hosted folding cannot reconstruct an unclassified CLI flag; its config selection was not copied");
    }
  }
  const expandTilde = (value: string) => value === "~" ? homedir()
    : value.startsWith("~/") ? path.join(homedir(), value.slice(2)) : value;
  const base = launchCwd !== undefined && path.isAbsolute(launchCwd) ? path.normalize(launchCwd) : undefined;
  const resolved = files.map((file) => {
    const expanded = expandTilde(file);
    // Settings retains startup-resolved overlays when a resumed session changes cwd.
    if (!path.isAbsolute(expanded) && (!base || !path.isAbsolute(base))) {
      throw new Error("Relaunch OMP with --config <absolute-path> and absolute PI_CONFIG_FILES entries, or --cwd <absolute-startup-directory> for relative config paths");
    }
    const absolute = path.isAbsolute(expanded) ? path.normalize(expanded) : path.resolve(base!, expanded);
    if (/[\r\n\0]/.test(absolute) || absolute.includes(path.delimiter) || Buffer.byteLength(absolute) > 4_096) {
      throw new Error("Hosted folding cannot represent this config path in OMP's PI_CONFIG_FILES contract");
    }
    return absolute;
  });
  if (resolved.length > 32 || Buffer.byteLength(resolved.join(path.delimiter)) > 16_384) {
    throw new Error("Hosted folding config selection exceeds 32 paths or 16 KiB");
  }
  return resolved;
}

export async function hostedRuntime(cwd: string, pi: any) {
  const executable = realpathSync(process.execPath);
  const script = process.argv[1];
  const entrypoint = script && path.isAbsolute(script) && existsSync(script) ? realpathSync(script) : undefined;
  const program = path.basename(executable).toLowerCase();
  const supported = entrypoint
    ? ["bun", "bun.exe"].includes(program) && ["cli.js", "cli.ts"].includes(path.basename(entrypoint))
    : ["omp", "omp.exe"].includes(program);
  if (!supported) {
    throw new Error("Hosted folding requires an OMP executable or its Bun CLI entrypoint with auth-gateway stdio support");
  }
  if (!path.isAbsolute(cwd) || /[\r\n\0]/.test(cwd)) throw new Error("Hosted folding requires the effective absolute OMP working directory");
  cwd = path.normalize(cwd);
  const overlays = await configFiles(pi);
  const agentDir = await configDirectory(executable, entrypoint, cwd, overlays);
  const environment: Record<string, string> = {
    PI_CODING_AGENT_DIR: agentDir,
    OMP_PROFILE: (process.env.OMP_PROFILE ?? process.env.PI_PROFILE)?.trim() || "default",
    PI_CONFIG_DIR: process.env.PI_CONFIG_DIR || ".omp",
  };
  return { executable, ...(entrypoint ? { entrypoint } : {}), home: realpathSync(homedir()), agentDir, cwd, configFiles: overlays, environment };
}

export async function foldOptions(words: string[], cwd: string, pi: any): Promise<Record<string, unknown>> {
  if (!words.length) return {};
  if (words[0] === "hosted") {
    const model = words[1];
    if (words.length > 2 || (model !== undefined && !/^[a-zA-Z0-9_.-]+\/[a-zA-Z0-9_./-]+$/.test(model))) {
      throw new Error("Hosted folding requires an exact provider/model selector");
    }
    return { backend: "hosted", ...(model === undefined ? {} : { model }), runtime: await hostedRuntime(cwd, pi) };
  }
  const model = words[0] === "local" ? words[1] : words[0];
  if (!model || words.length !== (words[0] === "local" ? 2 : 1)) {
    throw new Error("Use /orihon fold local MODEL:TAG or /orihon fold hosted");
  }
  return { backend: "local", model };
}

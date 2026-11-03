// Loaded by compat/cutover-smoke.ts into one real OMP process. It resolves the
// runner's disposable installed tree through the production loader, installs
// the production portable-session module with the loader's real reconnect
// gate, attaches through the real `/spirit` handler, and publishes the
// attachment state every 200 ms.
import { writeFileSync } from "node:fs";

import { configureInstalledAthanor, reconnectGate } from "../installed-loader.ts";
import { __sessionTest, installPortableSession } from "../house-proof/session.ts";

export default function cutoverProbe(pi: any) {
  const receipt = String(process.env.ATHANOR_CUTOVER_SMOKE_RECEIPT);
  const options = {
    programRoot: String(process.env.ATHANOR_CUTOVER_SMOKE_PROGRAM_ROOT),
    userProfile: String(process.env.ATHANOR_CUTOVER_SMOKE_PROFILE),
  };
  // Projects the Host token and URL from the tree's client.json into this
  // process, exactly as the installed loader does before it imports index.ts.
  const loaded = configureInstalledAthanor(options);

  let spirit: ((args: string, ctx: any) => Promise<void>) | null = null;
  const facade = new Proxy(pi, {
    get(target, key) {
      if (key === "registerCommand") {
        return (name: string, command: any) => {
          if (name === "spirit") spirit = command.handler;
          return target.registerCommand(name, command);
        };
      }
      const value = Reflect.get(target, key);
      return typeof value === "function" ? value.bind(target) : value;
    },
  });
  installPortableSession(facade, { reconnectGate: reconnectGate(loaded, options) });

  pi.on("session_start", async (_event: unknown, ctx: any) => {
    let attachError: string | null = null;
    try {
      await spirit?.("kintsu", ctx);
    } catch (error) {
      attachError = error instanceof Error ? error.message : String(error);
    }
    const key = __sessionTest.conversationKey(ctx);
    const publish = () => {
      const attachment = key ? __sessionTest.attachments.get(key) : undefined;
      writeFileSync(receipt, JSON.stringify({
        pid: process.pid,
        sessionFile: ctx?.sessionManager?.getSessionFile?.() ?? null,
        attachError,
        status: attachment?.status ?? null,
        connected: attachment?.connected ?? null,
        error: attachment?.error ?? null,
        hostToken: process.env.ATHANOR_HOST_TOKEN ?? null,
      }));
    };
    publish();
    setInterval(publish, 200);
  });
}

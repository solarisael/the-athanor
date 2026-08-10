import { describe, expect, test } from "bun:test";
import { createOperationTarget, diagnosticView, operationId, renderOperation } from "../gui/app.js";

class Element {
  children: Element[] = [];
  afterChildren: Element[] = [];
  dataset: Record<string, string> = {};
  className = "";
  id = "";
  textContent = "";
  type = "";
  open = false;
  onclick: (() => Promise<void>) | undefined;
  ownerDocument: DocumentStub;
  constructor(readonly tagName: string, document: DocumentStub) { this.ownerDocument = document; }
  append(...children: Element[]) { this.children.push(...children); }
  after(child: Element) { this.afterChildren.push(child); }
  replaceChildren(...children: Element[]) { this.children = children; }
  setAttribute() {}
}
class DocumentStub {
  createElement(tagName: string) { return new Element(tagName, this); }
}

const canonical = {
  code: "DATABASE_WRITE_FAILED",
  message: "Write rolled back",
  retryable: true,
  details: {
    owner: { component: "substrate", path: "src/store.rs", symbol: "write_memory" },
    evidence: [{ severity: "warning", summary: "transaction was rolled back" }],
    targets: ["src/store.rs#write_memory"],
    next_checks: [{ action: "inspect", target: "src/store.rs#write_memory" }],
    execution: { request_dispatched: true, write_outcome: "rolled_back", retry: "safe_now" },
  },
};

describe("GUI canonical diagnostic presentation", () => {
  test("classifies success, degraded, failed, and unknown outcomes", () => {
    expect(diagnosticView({ ok: true, result: { saved: true } }).kind).toBe("success");
    expect(diagnosticView({ ok: true, result: canonical }).kind).toBe("degraded");
    expect(diagnosticView({ ok: false, error: canonical }).kind).toBe("failed");
    const unknown = diagnosticView({ ok: false, error: { ...canonical, code: "AUTHORITATIVE_OUTCOME_UNKNOWN", details: { ...canonical.details, execution: { request_dispatched: true, write_outcome: "unknown", retry: "reconcile_first" } } } });
    expect(unknown.kind).toBe("unknown");
    expect(unknown.summary).toContain("reconcile");
  });

  test("preserves the canonical packet and exposes its raw diagnostic fields", () => {
    const view = diagnosticView({ ok: false, error: canonical });
    expect(view.rawPacket).toBe(canonical);
    expect(view.rawText).toContain('"owner"');
    expect(view.rawText).toContain('"evidence"');
    expect(view.rawText).toContain('"targets"');
    expect(view.rawText).toContain('"next_checks"');
    expect(view.rawText).toContain('"execution"');
  });

  test("creates independently addressable, expandable targets with visible raw copy control", () => {
    const document = new DocumentStub();
    const firstHost = document.createElement("button");
    const secondHost = document.createElement("button");
    const first = createOperationTarget(document as any, firstHost as any, operationId("remember", 0));
    const second = createOperationTarget(document as any, secondHost as any, operationId("recall", 0));
    renderOperation(first as any, { ok: false, error: canonical });
    expect(first.container.id).not.toBe(second.container.id);
    expect(firstHost.afterChildren[0]).toBe(first.container);
    expect(first.raw.tagName).toBe("details");
    expect(first.raw.open).toBe(false);
    first.raw.open = true;
    expect(first.raw.open).toBe(true);
    expect(first.copy.textContent).toBe("Copy raw JSON");
    expect(first.pre.textContent).toContain('"owner"');
  });
});

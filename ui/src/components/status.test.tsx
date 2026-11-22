// The components that say how a run or the host stands, each in each of its states.
import { describe, expect, it } from "vitest";
import { expectShownAsText, HOSTILE, render } from "../test/render";
import { AllowanceBar } from "./AllowanceBar";
import { CostTable, costTotal } from "./CostTable";
import { PlanBlock, needFigures } from "./PlanBlock";
import { ProgressBar } from "./ProgressBar";
import { SpendMeter } from "./SpendMeter";
import { StatusBar } from "./StatusBar";
import { StepList } from "./StepList";
import { Tag, tagWords } from "./Tag";
import { VerdictCard } from "./VerdictCard";

describe("Tag", () => {
  it("says each kind", () => {
    expect(tagWords({ kind: "draft" })).toBe("DRAFT");
    expect(tagWords({ kind: "running", progress: 0.4 })).toBe("RUNNING · 40%");
    expect(tagWords({ kind: "running" })).toBe("RUNNING");
    expect(tagWords({ kind: "pass", detail: "2 tries" })).toBe("PASS · 2 TRIES");
    expect(tagWords({ kind: "fail" })).toBe("FAIL");
    expect(tagWords({ kind: "unsure" })).toBe("UNSURE");
    expect(tagWords({ kind: "no-key" })).toBe("NO KEY");
    expect(tagWords({ kind: "interrupted" })).toBe("INTERRUPTED");
    expect(tagWords({ kind: "free-local" })).toBe("FREE · LOCAL");
  });
  it("renders filled and soft", () => {
    const view = render(<Tag kind="pass" />);
    expect(view.get("span").className).toBe("tag tag--pass mono");
    view.rerender(<Tag kind="unsure" soft />);
    expect(view.get("span").className).toContain("tag--soft");
  });
});

describe("StatusBar", () => {
  it("says ready, the store, each backend and the version", () => {
    const view = render(
      <StatusBar
        ready
        store="/data/whizbang"
        version="0.1.0"
        backends={[
          { name: "SAM3", ready: true },
          { name: "HAIKU", ready: false, why: "no key" },
        ]}
      />,
    );
    expect(view.text()).toBe("● ready · store /data/whizbang · SAM3 ● · HAIKU no keywhizbang 0.1.0");
  });
  it("says a problem instead, as text", () => {
    const view = render(<StatusBar ready={false} backends={[]} problem={HOSTILE} />);
    expectShownAsText(view);
  });
});

describe("VerdictCard", () => {
  it("renders pass, fail and unsure with their words and reasons as text", () => {
    for (const verdict of ["pass", "fail", "unsure"] as const) {
      const view = render(<VerdictCard verdict={verdict} title="Check passed" detail="try 2 · Gemini" reasons={[HOSTILE, "the hose end is gone"]} />);
      expect(view.get("section").className).toBe(`verdict verdict--${verdict}`);
      expect(view.all("li")).toHaveLength(2);
      expectShownAsText(view);
      expect(view.text().startsWith(verdict === "pass" ? "✓" : "Check")).toBe(true);
      view.unmount();
    }
  });
});

describe("ProgressBar, SpendMeter, AllowanceBar, CostTable", () => {
  it("renders a bar how far, and one that only says it is going", () => {
    const view = render(<ProgressBar label="Depth" value={0.78} title="Depth" figures="3 of 4 files" />);
    expect(view.get("[role=progressbar]").getAttribute("aria-valuenow")).toBe("78");
    expect(view.text()).toBe("Depth3 of 4 files");
    view.rerender(<ProgressBar label="fill running" />);
    expect(view.get("[role=progressbar]").className).toContain("progress--indeterminate");
    expect(view.get("[role=progressbar]").getAttribute("aria-valuenow")).toBeNull();
  });
  it("says spent of reserved", () => {
    const view = render(<SpendMeter spent={0.046} reserved={0.14} />);
    expect(view.text()).toBe("Spent so far$0.046 of up to $0.140");
    expect(view.get("[role=meter]").getAttribute("aria-valuenow")).toBe("33");
    view.rerender(<SpendMeter spent={0} reserved={0} />);
    expect(view.get("[role=meter]").getAttribute("aria-valuenow")).toBe("0");
    view.rerender(<SpendMeter title="Settled so far" spent={0.04} reserved={0.14} />);
    expect(view.text()).toBe("Settled so far$0.040 of up to $0.140");
    expect(view.get("[role=meter]").getAttribute("aria-label")).toBe("Settled so far");
  });
  it("says the allowance since the last reset and what is left", () => {
    const view = render(<AllowanceBar allowance={10} spent={2.48} since="since 2026-11-02" />);
    expect(view.text()).toBe("$2.48 since 2026-11-02$7.52 left of $10.00");
    expect(view.get("[role=meter]").getAttribute("aria-valuenow")).toBe("25");
    view.rerender(<AllowanceBar allowance={1} spent={3} since="since x" />);
    expect(view.text()).toContain("$0.000 left of $1.00");
  });
  it("splits cost by capability and backend, free local work as free", () => {
    const lines = [
      { what: "fill × 2", backend: "FAL", cost: 0.08 },
      { what: "find", backend: "SAM3 local", cost: 0 },
      { what: "judge", backend: HOSTILE, cost: 0.008 },
    ];
    expect(costTotal(lines)).toBeCloseTo(0.088);
    const view = render(<CostTable lines={lines} />);
    expect(view.all("tbody td").map((td) => td.textContent)).toEqual(["$0.080", "free", "$0.008"]);
    expect(view.get("tfoot td").textContent).toBe("$0.088");
    expectShownAsText(view);
  });
});

describe("PlanBlock", () => {
  const needs = [
    { capability: "segment", backend: "sam3", requests: 17, each: 0, total: 0, local: true },
    { capability: "fill", backend: "fal", model: "bria-eraser", requests: 3, each: 0.04, total: 0.12 },
    { capability: "judge", backend: "gemini", requests: 1, each: 0.002, total: 0.002 },
  ];
  it("says each need's figures", () => {
    expect(needFigures(needs[0]!)).toBe("17 req · local");
    expect(needFigures(needs[1]!)).toBe("3 × ≤ $0.040");
    expect(needFigures(needs[2]!)).toBe("1 × $0.002");
  });
  it("lists the needs by capability, backend and model, a loop's attempts, and the bound", () => {
    const view = render(<PlanBlock needs={needs} bound={3.18} attempts={{ asked: 3, allowed: 3 }} />);
    expect(view.all(".plan-block__name").map((el) => el.textContent)).toEqual(["segment · sam3", "fill · fal bria-eraser", "judge · gemini", "attempts", "held while running"]);
    expect(view.get(".plan-block__total").textContent).toBe("up to $3.18");
    view.rerender(<PlanBlock needs={[needs[0]!]} bound={0} stale />);
    expect(view.get(".plan-block__total").textContent).toBe("free");
    expect(view.get("section").getAttribute("aria-busy")).toBe("true");
  });
});

describe("StepList", () => {
  it("renders done, failed, running and waiting steps, an attempt divider and a gap", () => {
    const view = render(
      <StepList
        entries={[
          { kind: "step", key: "1", name: "find", state: "done", ms: 4100, where: "local", note: "SAM3 found 1 hose" },
          { kind: "step", key: "2", name: "judge", state: "failed", ms: 3000, where: "$0.002", note: HOSTILE },
          { kind: "attempt", key: "a2", number: 2, change: "widen the mask" },
          { kind: "skipped", key: "s", count: 12 },
          { kind: "step", key: "3", name: "fill", state: "running", ms: 6200 },
          { kind: "step", key: "4", name: "judge", state: "waiting" },
        ]}
      />,
    );
    expectShownAsText(view);
    expect(view.all("[data-state]").map((el) => el.getAttribute("data-state"))).toEqual(["done", "failed", "running", "waiting"]);
    expect(view.all(".step-list__glyph").map((el) => el.textContent)).toEqual(["✓", "✕", "◐", "○"]);
    expect(view.get("[role=separator]").textContent).toBe("ATTEMPT 2 · widen the mask");
    expect(view.get(".step-list__skipped").textContent).toBe("12 messages not kept");
    expect(view.all(".step-list__figures").map((el) => el.textContent)).toEqual(["4.1 s · local", "3.0 s · $0.002", "6.2 s …"]);
    expect(view.all("[role=progressbar]")).toHaveLength(1);
  });
});

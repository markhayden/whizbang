// The components that show pictures and the rows that list them, each in each state.
import { describe, expect, it, vi } from "vitest";
import { act } from "preact/test-utils";
import { click, expectShownAsText, HOSTILE, render } from "../test/render";
import { boxOf, Canvas, stepZoom, toPicture } from "./Canvas";
import { Compare, differenceOf, fitZoom } from "./Compare";
import { HistoryRow } from "./HistoryRow";
import { ModelRow } from "./ModelRow";

describe("Canvas", () => {
  it("maps a pointer to the picture's pixels, held to the picture", () => {
    const rect = { left: 10, top: 20, width: 200, height: 100 };
    expect(toPicture(110, 70, rect, 400, 200)).toEqual([200, 100]);
    expect(toPicture(0, 0, rect, 400, 200)).toEqual([0, 0]);
    expect(toPicture(999, 999, rect, 400, 200)).toEqual([400, 200]);
    expect(boxOf([30, 40], [10, 5])).toEqual({ x: 10, y: 5, w: 20, h: 35 });
  });
  it("steps the zoom in and out", () => {
    expect(stepZoom(1, 1)).toBe(2);
    expect(stepZoom(1, -1)).toBe(0.5);
    expect(stepZoom(4, 1)).toBe(4);
    expect(stepZoom(0.25, -1)).toBe(0.25);
  });
  it("draws each mark over the picture, and the words of a flag as text", () => {
    const view = render(
      <Canvas
        src="/p.png"
        name="dog.png"
        width={640}
        height={480}
        zoom="fit"
        marks={[
          { kind: "box", x: 10, y: 20, w: 100, h: 50, label: "box 1 · 100×50" },
          { kind: "mask", src: "/mask.png", label: "mask 1 · 94%" },
          { kind: "point", x: 300, y: 200 },
          { kind: "stroke", points: [[0, 0], [10, 10], [20, 5]], width: 12 },
          { kind: "flag", x: 50, y: 60, w: 70, h: 80, words: HOSTILE },
        ]}
      />,
    );
    expectShownAsText(view);
    expect(view.get("svg").getAttribute("viewBox")).toBe("0 0 640 480");
    expect(view.all(".canvas__box")).toHaveLength(1);
    expect(view.all(".canvas__mask")).toHaveLength(1);
    expect(view.all("mask image")).toHaveLength(1);
    expect(view.all(".canvas__point")).toHaveLength(1);
    expect(view.get(".canvas__stroke").getAttribute("points")).toBe("0,0 10,10 20,5");
    expect(view.all(".canvas__flag")).toHaveLength(1);
    expect(view.all(".canvas__label").map((el) => el.textContent)).toEqual(["box 1 · 100×50", "mask 1 · 94%", HOSTILE]);
    expect(view.get(".canvas__fit").textContent).toBe("fit");
  });
  it("zooms by a scale and back to fit", () => {
    const onZoom = vi.fn();
    const view = render(<Canvas src="/p.png" name="p" width={100} height={50} zoom={2} marks={[]} onZoom={onZoom} />);
    expect(view.get<HTMLElement>(".canvas__inner").style.width).toBe("200px");
    expect(view.get(".canvas__fit").textContent).toBe("200%");
    click(view.get("[aria-label='Zoom in']"));
    expect(onZoom).toHaveBeenLastCalledWith(4);
    click(view.get(".canvas__fit"));
    expect(onZoom).toHaveBeenLastCalledWith("fit");
  });
  it("takes a drawn box with the box tool, and nothing without a tool", () => {
    const onDraw = vi.fn();
    const view = render(<Canvas src="/p.png" name="p" width={100} height={100} zoom={1} marks={[]} tool="box" onDraw={onDraw} />);
    const svg = view.get<SVGSVGElement>("svg");
    svg.getBoundingClientRect = () => ({ left: 0, top: 0, width: 100, height: 100, right: 100, bottom: 100, x: 0, y: 0, toJSON: () => ({}) });
    const fire = (type: string, x: number, y: number) =>
      act(() => {
        svg.dispatchEvent(new PointerEvent(type, { clientX: x, clientY: y, bubbles: true }));
      });
    fire("pointerdown", 10, 10);
    fire("pointermove", 40, 30);
    expect(view.all(".canvas__box--live")).toHaveLength(1);
    fire("pointerup", 40, 30);
    expect(onDraw).toHaveBeenCalledWith({ kind: "box", x: 10, y: 10, w: 30, h: 20 });
    view.rerender(<Canvas src="/p.png" name="p" width={100} height={100} zoom={1} marks={[]} onDraw={onDraw} />);
    fire("pointerdown", 10, 10);
    fire("pointerup", 50, 50);
    expect(onDraw).toHaveBeenCalledTimes(1);
  });
  it("takes a point with a click", () => {
    const onDraw = vi.fn();
    const view = render(<Canvas src="/p.png" name="p" width={100} height={100} zoom={1} marks={[]} tool="point" onDraw={onDraw} />);
    const svg = view.get<SVGSVGElement>("svg");
    svg.getBoundingClientRect = () => ({ left: 0, top: 0, width: 50, height: 50, right: 50, bottom: 50, x: 0, y: 0, toJSON: () => ({}) });
    act(() => {
      svg.dispatchEvent(new PointerEvent("pointerdown", { clientX: 10, clientY: 20, bubbles: true }));
    });
    act(() => {
      svg.dispatchEvent(new PointerEvent("pointerup", { clientX: 10, clientY: 20, bubbles: true }));
    });
    expect(onDraw).toHaveBeenCalledWith({ kind: "point", x: 20, y: 40 });
  });
});

describe("Compare", () => {
  it("computes the difference of two pictures, opaque and amplified", () => {
    const a = new Uint8ClampedArray([10, 20, 30, 255, 0, 0, 0, 255]);
    const b = new Uint8ClampedArray([12, 20, 25, 128, 200, 0, 0, 255]);
    expect(Array.from(differenceOf(a, b))).toEqual([8, 0, 20, 255, 255, 0, 0, 255]);
  });
  it("shows side by side, swipe and difference at one zoom", () => {
    const onMode = vi.fn();
    const view = render(<Compare before="/a.png" after="/b.png" width={64} height={32} mode="side" zoom={2} onMode={onMode} />);
    const figures = view.all<HTMLElement>("figure");
    expect(figures).toHaveLength(2);
    expect(figures.map((f) => f.style.width)).toEqual(["128px", "128px"]);
    expect(view.all(".compare__badge").map((el) => el.textContent)).toEqual(["BEFORE", "AFTER"]);
    click(view.all("[role=tab]")[1]!);
    expect(onMode).toHaveBeenCalledWith("swipe");
    view.rerender(<Compare before="/a.png" after="/b.png" width={64} height={32} mode="swipe" zoom={2} />);
    expect(view.all("img")).toHaveLength(2);
    expect(view.get<HTMLElement>(".compare__top").style.clipPath).toBe("inset(0 50% 0 0)");
    expect(view.get<HTMLElement>("figure").style.width).toBe("128px");
    view.rerender(<Compare before="/a.png" after="/b.png" width={64} height={32} mode="difference" zoom={1} />);
    expect(view.get("canvas").getAttribute("width")).toBe("64");
    expect(view.get<HTMLElement>("figure").style.width).toBe("64px");
  });
  it("fits two large pictures side by side in the stage, at one zoom", () => {
    // 2400 by 1792 in a stage 790 wide: each a little under half of it, both inside it.
    const zoom = fitZoom(2400, 1792, { width: 790, height: 600 });
    expect(2 * Math.floor(2400 * zoom) + 2).toBeLessThanOrEqual(790);
    expect(Math.floor(1792 * zoom)).toBeLessThanOrEqual(600);
    // A short stage holds them to its height; a small picture is not blown up past four.
    expect(Math.floor(1792 * fitZoom(2400, 1792, { width: 4000, height: 300 }))).toBeLessThanOrEqual(300);
    expect(fitZoom(16, 16, { width: 4000, height: 4000 })).toBe(4);
    const view = render(<Compare before="/a.png" after="/b.png" width={2400} height={1792} mode="side" zoom="fit" />);
    const widths = view.all<HTMLElement>("figure").map((f) => parseInt(f.style.width, 10));
    expect(widths[0]).toBe(widths[1]);
    expect(widths[0]! * 2).toBeLessThanOrEqual(960);
  });
});

describe("HistoryRow", () => {
  it("renders a run, default and selected, its text as text", () => {
    const onOpen = vi.fn();
    const view = render(
      <HistoryRow task="Remove, checked" tone="copper" asked={HOSTILE} tag={{ kind: "pass", detail: "2 tries" }} models="FAL · Gemini" time="21.4 s" cost={0.088} when="just now" onOpen={onOpen} />,
    );
    expectShownAsText(view);
    expect(view.text()).toContain("PASS · 2 TRIES");
    expect(view.text()).toContain("$0.088");
    expect(view.get(".tag").className).toContain("tag--soft");
    click(view.get("button"));
    expect(onOpen).toHaveBeenCalled();
    view.rerender(<HistoryRow task="Reality check" tone="volt" asked="kitchen-wide.png" tag={{ kind: "unsure" }} cost={0.002} when="8m ago" selected />);
    expect(view.get("button").className).toContain("history-row--selected");
  });
});

describe("ModelRow", () => {
  it("renders ready, get, installing, key set, no key and not set up, each with its one action", () => {
    const onGet = vi.fn();
    const ready = render(<ModelRow name="SAM3" line="finds things you name" size={3_400_000_000} state={{ kind: "ready" }} onRemove={() => undefined} />);
    expect(ready.text()).toBe("SAM3finds things you name3.4 GBREADYRemove");
    const get = render(<ModelRow name="tiny" line={HOSTILE} size={1_000} state={{ kind: "get" }} onGet={onGet} />);
    expectShownAsText(get);
    click(get.get("button"));
    expect(onGet).toHaveBeenCalled();
    const installing = render(<ModelRow name="SAM3" line="x" state={{ kind: "installing", filesDone: 3, files: 4 }} onCancel={() => undefined} />);
    expect(installing.text()).toContain("3 of 4 files");
    expect(installing.get("[role=progressbar]").getAttribute("aria-valuenow")).toBe("75");
    expect(installing.get("button").textContent).toBe("Cancel");
    const keySet = render(<ModelRow name="Gemini" line="judges · ~$0.002 a check" state={{ kind: "key-set" }} />);
    expect(keySet.text()).toBe("Geminijudges · ~$0.002 a checkKEY SET");
    const noKey = render(<ModelRow name="Claude" line="judges" state={{ kind: "no-key" }} onHowToAdd={() => undefined} />);
    expect(noKey.text()).toBe("ClaudejudgesNO KEYHow to add");
    const notSetUp = render(<ModelRow name="OpenAI" line="judges" state={{ kind: "not-set-up" }} />);
    expect(notSetUp.text()).toBe("OpenAIjudgesNOT SET UP");
  });
});

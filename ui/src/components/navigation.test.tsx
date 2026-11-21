// The components that find and move between kits, tasks and runs, each in each state.
import { describe, expect, it, vi } from "vitest";
import { act } from "preact/test-utils";
import { click, expectShownAsText, HOSTILE, input, render } from "../test/render";
import { BrandArt } from "./BrandArt";
import { Breadcrumb } from "./Breadcrumb";
import { ContinueRow } from "./ContinueRow";
import { KitPanel, kitCounts } from "./KitPanel";
import { KitSwitcher, kitKey } from "./KitSwitcher";
import { kitTone } from "./kitTone";
import { SearchPalette, matching } from "./SearchPalette";
import { SendToMenu } from "./SendToMenu";
import { SentFromBanner } from "./SentFromBanner";
import { TaskRow } from "./TaskRow";
import { TopBar } from "./TopBar";
import { Wordmark } from "./Wordmark";

describe("kitTone", () => {
  it("gives rascal's and squint's places copper and volt, and any later kit a tone", () => {
    expect(kitTone(0)).toBe("copper");
    expect(kitTone(1)).toBe("volt");
    expect(kitTone(2)).toBe("unsure");
    expect(kitTone(7)).toBe("unsure");
  });
});

describe("KitPanel and TaskRow", () => {
  const tasks = [
    { title: "Remove what you name", shape: "image + words → image", href: "#/k/rascal/remove_prompt" },
    { title: "Remove, checked", shape: "image + words → image", uses: "Squint", href: "#/k/rascal/remove_checked" },
    { title: HOSTILE, shape: "image → mask" },
  ];
  it("counts tasks and tools", () => {
    expect(kitCounts(6, 40)).toBe("6 tasks · 40 tools");
    expect(kitCounts(1)).toBe("1 task");
  });
  it("renders the kit, its tasks, and the rest counted", () => {
    const onMore = vi.fn();
    const view = render(<KitPanel name="Rascal" line="edit pictures" tone="copper" tasks={tasks} tools={40} shown={2} onMore={onMore} />);
    expect(view.get("h2").textContent).toBe("Rascal");
    expect(view.get(".kit-panel__mark").className).toContain("tone-fill--copper");
    expect(view.all(".task-row")).toHaveLength(2);
    expect(view.get("a.task-row").getAttribute("href")).toBe("#/k/rascal/remove_prompt");
    expect(view.all(".task-row")[1]!.textContent).toContain("· uses Squint");
    click(view.get(".kit-panel__more"));
    expect(onMore).toHaveBeenCalled();
    expect(view.get(".kit-panel__more").textContent).toBe("+1 task ›");
  });
  it("shows a task title as text, and a kit with no tasks", () => {
    const view = render(<TaskRow title={HOSTILE} shape="image → mask" />);
    expectShownAsText(view);
    view.rerender(<KitPanel name="Pixel" line="make pictures" tone="unsure" tasks={[]} />);
    expect(view.text()).toContain("This kit has no tasks in this build.");
  });
});

describe("ContinueRow", () => {
  it("renders a run going and a draft", () => {
    const onOpen = vi.fn();
    const view = render(<ContinueRow title="Reality check" what={`squint · ${HOSTILE}`} tag={{ kind: "running", progress: 0.4 }} when="now" onOpen={onOpen} />);
    expectShownAsText(view);
    expect(view.text()).toContain("RUNNING · 40%");
    click(view.get("button"));
    expect(onOpen).toHaveBeenCalled();
    view.rerender(<ContinueRow title="Remove, checked" what="rascal · dog.jpg" tag={{ kind: "draft" }} when="12m ago" thumb="/t.png" />);
    expect(view.text()).toContain("DRAFT");
    expect(view.get("img").getAttribute("src")).toBe("/t.png");
  });
});

describe("Breadcrumb", () => {
  it("renders the kit and the task, each opening its menu", () => {
    const onKit = vi.fn();
    const onSearch = vi.fn();
    const view = render(<Breadcrumb kit="Rascal" tone="copper" task="Remove, checked" onKit={onKit} onSearch={onSearch} />);
    expect(view.text()).toBe("Rascal▾/Remove, checked▾⌘K");
    click(view.get(".breadcrumb__kit"));
    click(view.get(".breadcrumb__search"));
    expect(onKit).toHaveBeenCalled();
    expect(onSearch).toHaveBeenCalled();
    view.rerender(<Breadcrumb kit="Squint" tone="volt" />);
    expect(view.all(".breadcrumb__task")).toHaveLength(0);
  });
});

describe("KitSwitcher", () => {
  it("lists the kits with ⌘1 to ⌘9 and Home with ⌘0, the current one marked", () => {
    expect(kitKey(0)).toBe("⌘1");
    expect(kitKey(8)).toBe("⌘9");
    expect(kitKey(9)).toBeUndefined();
    const onPick = vi.fn();
    const onHome = vi.fn();
    const view = render(
      <KitSwitcher
        kits={[
          { id: "rascal", name: "Rascal", line: "Edit pictures", tone: "copper" },
          { id: "squint", name: "Squint", line: "Check pictures", tone: "volt" },
        ]}
        current="rascal"
        note="same picture stays open"
        onPick={onPick}
        onHome={onHome}
      />,
    );
    expect(view.all(".menu__line").map((el) => el.textContent)).toEqual(["Edit pictures · current", "Check pictures · same picture stays open"]);
    expect(view.all(".menu__key").map((el) => el.textContent)).toEqual(["⌘1", "⌘2", "⌘0"]);
    click(view.all(".menu__item")[1]!);
    expect(onPick).toHaveBeenCalledWith("squint");
    click(view.get(".menu__foot"));
    expect(onHome).toHaveBeenCalled();
  });
});

describe("SendToMenu", () => {
  it("lists each target, copy and download", () => {
    const onSend = vi.fn();
    const onCopy = vi.fn();
    const view = render(
      <SendToMenu
        targets={[
          { id: "squint/check", title: "Squint · Check an edit", line: "Source and this result filled in", tone: "volt" },
          { id: "rascal/remove", title: HOSTILE, line: "Keeps this edit protected", tone: "copper" },
        ]}
        active="squint/check"
        onSend={onSend}
        onCopyCommand={onCopy}
        onDownload={() => undefined}
      />,
    );
    expectShownAsText(view);
    expect(view.all(".menu__item--active")).toHaveLength(1);
    click(view.all(".menu__item")[0]!);
    expect(onSend).toHaveBeenCalledWith("squint/check");
    click(view.all(".link-button")[0]!);
    expect(onCopy).toHaveBeenCalled();
  });
  it("says when no task takes the result", () => {
    const view = render(<SendToMenu targets={[]} />);
    expect(view.text()).toContain("No task takes this result.");
  });
});

describe("SentFromBanner", () => {
  it("says where the draft came from and goes back", () => {
    const onBack = vi.fn();
    const view = render(<SentFromBanner title={HOSTILE} line="Source and Edited are filled in." tone="copper" onBack={onBack} />);
    expectShownAsText(view);
    click(view.get("button"));
    expect(onBack).toHaveBeenCalled();
  });
});

describe("SearchPalette", () => {
  const entries = [
    { key: "a", kind: "task" as const, label: "Remove what you name", detail: "Rascal" },
    { key: "b", kind: "task" as const, label: "Check an edit", detail: "Squint" },
    { key: "c", kind: "picture" as const, label: HOSTILE },
  ];
  it("matches every word without case", () => {
    expect(matching(entries, "remove RASCAL").map((e) => e.key)).toEqual(["a"]);
    expect(matching(entries, "").map((e) => e.key)).toEqual(["a", "b", "c"]);
    expect(matching(entries, "", 1)).toHaveLength(1);
  });
  it("filters as it is typed, picks with Enter, closes with Escape", () => {
    const onPick = vi.fn();
    const onClose = vi.fn();
    const view = render(<SearchPalette entries={entries} onPick={onPick} onClose={onClose} />);
    expectShownAsText(view);
    const box = view.get<HTMLInputElement>("input");
    input(box, "check");
    expect(view.all("[role=option]")).toHaveLength(1);
    act(() => {
      box.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
    });
    expect(onPick).toHaveBeenCalledWith("b");
    input(box, "nothing like it");
    expect(view.text()).toContain("Nothing matches.");
    act(() => {
      box.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    });
    expect(onClose).toHaveBeenCalled();
  });
});

describe("TopBar, Wordmark and BrandArt", () => {
  it("renders the wordmark, where you are, search, the pages and the allowance", () => {
    const onSearch = vi.fn();
    const view = render(<TopBar where={<span>Home</span>} left={7.61} allowance={10} current="history" homeHref="#/" historyHref="#/history" modelsHref="#/models" onSearch={onSearch} />);
    expect(view.text()).toBe("whizbangHomeSearch tasks, tools and pictures⌘KHistoryModels$7.61 / 10.00");
    expect(view.get(".top-bar__link--current").textContent).toBe("History");
    click(view.get(".top-bar__search"));
    expect(onSearch).toHaveBeenCalled();
    view.rerender(<TopBar where={null} homeHref="#/" historyHref="#/history" modelsHref="#/models" />);
    expect(view.all(".top-bar__allowance")).toHaveLength(0);
  });
  it("sets the wordmark in type until the art is committed, then shows the art", () => {
    const view = render(<Wordmark />);
    expect(view.get(".wordmark--type").textContent).toBe("whizbang");
    view.rerender(<Wordmark src="/brand/wordmark.svg" size="home" />);
    expect(view.get("img").getAttribute("alt")).toBe("whizbang");
  });
  it("leaves an empty frame where the art goes, and shows the art once there", () => {
    const view = render(<BrandArt kind="crew" alt="" />);
    expect(view.get("[data-brand-slot=crew]").className).toContain("brand-art--empty");
    view.rerender(<BrandArt kind="emblem" alt="the emblem" src="/brand/emblem.webp" />);
    expect(view.get("img").getAttribute("alt")).toBe("the emblem");
  });
});

// The input components, each in each of its states.
import { describe, expect, it, vi } from "vitest";
import { click, expectShownAsText, HOSTILE, input, render } from "../test/render";
import { BindingPicker } from "./BindingPicker";
import { Button } from "./Button";
import { ChoiceList, readinessMark } from "./ChoiceList";
import { DropTarget, picturesOf } from "./DropTarget";
import { ListField, countLine } from "./ListField";
import { NumberField, readNumber } from "./NumberField";
import { PictureField } from "./PictureField";
import { PictureTile } from "./PictureTile";
import { Segmented } from "./Segmented";
import { TextField } from "./TextField";

describe("Button", () => {
  it("renders each variant with its class, and a key hint", () => {
    for (const variant of ["primary", "secondary", "quiet", "danger"] as const) {
      const view = render(
        <Button variant={variant} hint="⌘↵">
          Run
        </Button>,
      );
      expect(view.get("button").className).toContain(`button--${variant}`);
      expect(view.text()).toBe("Run⌘↵");
      view.unmount();
    }
  });
  it("calls back on a click", () => {
    const onClick = vi.fn();
    const view = render(<Button onClick={onClick}>Run</Button>);
    click(view.get("button"));
    expect(onClick).toHaveBeenCalledTimes(1);
  });
  it("is disabled with its reason said below it, and hides its hint", () => {
    const onClick = vi.fn();
    const view = render(
      <Button variant="primary" hint="⌘↵" disabled why="Add a judge key to check." onClick={onClick}>
        Run
      </Button>,
    );
    expect(view.get<HTMLButtonElement>("button").disabled).toBe(true);
    expect(view.get("[role=note]").textContent).toBe("Add a judge key to check.");
    expect(view.text()).not.toContain("⌘↵");
    click(view.get("button"));
    expect(onClick).not.toHaveBeenCalled();
  });
  it("is busy, as Stopping…", () => {
    const view = render(
      <Button variant="danger" busy>
        Stopping…
      </Button>,
    );
    expect(view.get("button").getAttribute("aria-busy")).toBe("true");
    expect(view.get("button").className).toContain("button--busy");
  });
});

describe("TextField", () => {
  it("shows its label, placeholder and help, and passes what is typed", () => {
    const onInput = vi.fn();
    const view = render(<TextField id="what" label="What to remove" value="" placeholder="e.g. the red ball" help="default · placeholder" onInput={onInput} />);
    expect(view.get("label").getAttribute("for")).toBe("what");
    expect(view.get("input").getAttribute("placeholder")).toBe("e.g. the red ball");
    input(view.get<HTMLInputElement>("input"), "the blue collar");
    expect(onInput).toHaveBeenCalledWith("the blue collar");
  });
  it("shows an error instead of its help, and is disabled", () => {
    const view = render(<TextField id="what" label="What to remove" value="" help="h" error="Say what to remove, or draw where it is." />);
    expect(view.get("[role=alert]").textContent).toBe("Say what to remove, or draw where it is.");
    expect(view.get("input").getAttribute("aria-invalid")).toBe("true");
    view.rerender(<TextField id="what" label="Where" optional value="" disabled />);
    expect(view.get<HTMLInputElement>("input").disabled).toBe(true);
    expect(view.text()).toContain("· optional");
  });
  it("shows a value from a run as text", () => {
    const view = render(<TextField id="what" label={HOSTILE} value={HOSTILE} help={HOSTILE} />);
    expectShownAsText(view);
    expect(view.get<HTMLInputElement>("input").value).toBe(HOSTILE);
  });
});

describe("NumberField", () => {
  it("reads numbers and integers within their bounds", () => {
    expect(readNumber("", false)).toEqual({ value: null });
    expect(readNumber("2.5", false)).toEqual({ value: 2.5 });
    expect(readNumber("2.5", true).error).toBe("Type a whole number.");
    expect(readNumber("x", false).error).toBe("Type a number.");
    expect(readNumber("0", true, 1, 5).error).toBe("At least 1.");
    expect(readNumber("9", true, 1, 5).error).toBe("At most 5.");
  });
  it("says its bounds, passes a good value and refuses a bad one", () => {
    const onInput = vi.fn();
    const view = render(<NumberField id="n" label="Attempts" integer min={1} max={5} value={3} onInput={onInput} />);
    expect(view.text()).toContain("1 to 5");
    input(view.get<HTMLInputElement>("input"), "4");
    expect(onInput).toHaveBeenLastCalledWith(4);
    input(view.get<HTMLInputElement>("input"), "7");
    expect(view.get("[role=alert]").textContent).toBe("At most 5.");
    expect(onInput).toHaveBeenCalledTimes(1);
  });
});

describe("PictureField", () => {
  const picture = { id: "a1", src: "/p.png", name: HOSTILE, width: 64, height: 48 };
  it("offers a drop and the library when empty", () => {
    const onChoose = vi.fn();
    const view = render(<PictureField id="p" label="Picture" picture={null} onChoose={onChoose} />);
    expect(view.text()).toContain("Drop a picture");
    click(view.get(".link-button"));
    expect(onChoose).toHaveBeenCalled();
  });
  it("shows the chosen picture's name as text, its size, a tag of where it came from, and Clear", () => {
    const onClear = vi.fn();
    const view = render(<PictureField id="p" label="Source" picture={picture} from="FROM RASCAL" onClear={onClear} />);
    expectShownAsText(view);
    expect(view.text()).toContain("64×48");
    expect(view.text()).toContain("FROM RASCAL");
    click(view.get(".link-button"));
    expect(onClear).toHaveBeenCalled();
  });
  it("offers nothing to change when disabled", () => {
    const view = render(<PictureField id="p" label="Source" picture={picture} disabled />);
    expect(view.all(".link-button")).toHaveLength(0);
  });
});

describe("ListField", () => {
  const items = [
    { key: "a", label: "one.png" },
    { key: "b", label: HOSTILE },
    { key: "c", label: "three.png" },
  ];
  it("says its count", () => {
    expect(countLine(2, 1, 4)).toBe("2 · 1 to 4");
    expect(countLine(2, 2, 2)).toBe("2 of 2");
    expect(countLine(0, 1, null)).toBe("0 · at least 1");
  });
  it("lists items in order, moves and removes them, and shows each as text", () => {
    const onMove = vi.fn();
    const onRemove = vi.fn();
    const view = render(<ListField id="l" label="References" items={items} min={1} max={4} adder={<span class="adder">add</span>} onMove={onMove} onRemove={onRemove} />);
    expectShownAsText(view);
    expect(view.all(".list-field__index").map((el) => el.textContent)).toEqual(["1", "2", "3"]);
    const up = view.all<HTMLButtonElement>("button[aria-label^='Move'][aria-label$='up']");
    expect(up[0]!.disabled).toBe(true);
    click(up[1]!);
    expect(onMove).toHaveBeenCalledWith(1, 0);
    click(view.all("button[aria-label^='Remove']")[2]!);
    expect(onRemove).toHaveBeenCalledWith(2);
    expect(view.all(".adder")).toHaveLength(1);
  });
  it("hides the adder at its most and marks a list short of its least", () => {
    const view = render(<ListField id="l" label="References" items={items} min={4} max={3} adder={<span class="adder">add</span>} />);
    expect(view.all(".adder")).toHaveLength(0);
    expect(view.get("ol").getAttribute("data-short")).toBe("true");
  });
});

describe("DropTarget and PictureTile", () => {
  it("takes image files only, at most the limit", () => {
    const files = [new File(["a"], "a.png", { type: "image/png" }), new File(["b"], "b.txt", { type: "text/plain" }), new File(["c"], "c.jpg", { type: "image/jpeg" })];
    expect(picturesOf(files).map((f) => f.name)).toEqual(["a.png", "c.jpg"]);
    expect(picturesOf(files, 1).map((f) => f.name)).toEqual(["a.png"]);
  });
  it("renders its label, and disabled", () => {
    const view = render(<DropTarget label="Drop a picture" disabled />);
    expect(view.text()).toBe("Drop a picture");
    expect(view.get<HTMLInputElement>("input").disabled).toBe(true);
    expect(view.get("label").className).toContain("drop-target--disabled");
  });
  it("renders a tile default and selected, named by its file", () => {
    const view = render(<PictureTile src="/p.png" name={HOSTILE} />);
    expect(view.get("img").getAttribute("alt")).toBe(HOSTILE);
    expect(view.root.querySelector("img[onerror]")).toBeNull();
    view.rerender(<PictureTile src="/p.png" name="dog.png" selected onClick={() => undefined} />);
    expect(view.get("button").getAttribute("aria-pressed")).toBe("true");
    expect(view.get("button").className).toContain("picture-tile--selected");
  });
});

describe("ChoiceList", () => {
  it("marks each readiness kind", () => {
    expect(readinessMark("ready")).toEqual({ text: "READY", tone: "pass" });
    expect(readinessMark("no_key")).toEqual({ text: "NO KEY", tone: "lacquer" });
    expect(readinessMark("not_installed", 3_400_000_000)).toEqual({ text: "GET · 3.4 GB", tone: "volt" });
    expect(readinessMark("out_of_date", 1_000_000).text).toBe("UPDATE · 1.0 MB");
    expect(readinessMark("damaged").text).toBe("DAMAGED");
    expect(readinessMark("other").text).toBe("NOT READY");
  });
  it("renders the choices with their marks, and a choice that is not ready can be picked", () => {
    const onChange = vi.fn();
    const view = render(
      <ChoiceList
        label="Judge"
        name="judge"
        value="gemini"
        onChange={onChange}
        choices={[
          { value: "gemini", label: "Gemini 3.8 Flash", readiness: "ready" },
          { value: "haiku", label: HOSTILE, readiness: "no_key", why: "the env file names no key" },
          { value: "sam3", label: "SAM 3 · local", readiness: "not_installed", size: 3_400_000_000 },
        ]}
      />,
    );
    expectShownAsText(view);
    expect(view.all(".choice-list__mark").map((el) => el.textContent)).toEqual(["READY", "NO KEY", "GET · 3.4 GB"]);
    expect(view.all(".choice-list__row--selected")).toHaveLength(1);
    expect(view.all(".choice-list__row")[1]!.getAttribute("title")).toBe("the env file names no key");
    const radio = view.all<HTMLInputElement>("input")[1]!;
    expect(radio.disabled).toBe(false);
    radio.dispatchEvent(new Event("change", { bubbles: true }));
    expect(onChange).toHaveBeenCalledWith("haiku");
  });
});

describe("Segmented", () => {
  it("renders numbers and tabs, the selected one marked, and calls back", () => {
    const onChange = vi.fn();
    const view = render(<Segmented label="Attempts" look="numbers" value="3" segments={["1", "2", "3"].map((v) => ({ value: v, label: v }))} onChange={onChange} />);
    expect(view.get(".segmented").className).toContain("segmented--numbers");
    expect(view.all("[aria-selected=true]").map((el) => el.textContent)).toEqual(["3"]);
    click(view.all("button")[0]!);
    expect(onChange).toHaveBeenCalledWith("1");
    view.rerender(<Segmented label="View" value="result" segments={[{ value: "source", label: "Source" }, { value: "result", label: "Result" }]} />);
    expect(view.get(".segmented").className).toContain("segmented--tabs");
  });
});

describe("BindingPicker", () => {
  const bindings = [
    { id: "m1", type: "mask", run: "r-1", task: "Find", when: "now" },
    { id: "l1", type: "location", run: "r-2", task: HOSTILE, when: "8m ago" },
    { id: "b1", type: "band_profile", run: "r-3", task: "Profile", when: "1h ago" },
  ];
  it("offers only the outputs of the port's type, and calls back with one", () => {
    const onPick = vi.fn();
    const view = render(<BindingPicker type="location" bindings={bindings} value={null} onPick={onPick} />);
    expectShownAsText(view);
    expect(view.all("[role=option]")).toHaveLength(1);
    click(view.get("button"));
    expect(onPick).toHaveBeenCalledWith("l1");
  });
  it("takes any artifact type, and says when there is none", () => {
    const view = render(<BindingPicker type="band_profile" bindings={bindings} value="b1" />);
    expect(view.get("[aria-selected=true]").textContent).toContain("Profile");
    view.rerender(<BindingPicker type="series" bindings={bindings} value={null} />);
    expect(view.text()).toBe("No earlier run made a series yet.");
  });
});

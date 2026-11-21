import { describe, expect, it } from "vitest";
import { ago, bytes, dollars, elapsed, middle, percent } from "./format";

describe("format", () => {
  it("says dollars with three decimals under one and two above", () => {
    expect(dollars(0.04)).toBe("$0.040");
    expect(dollars(0)).toBe("$0.000");
    expect(dollars(3.18)).toBe("$3.18");
    expect(dollars(Number.NaN)).toBe("$—");
  });
  it("says sizes in decimal units", () => {
    expect(bytes(3_400_000_000)).toBe("3.4 GB");
    expect(bytes(820_000_000)).toBe("820 MB");
    expect(bytes(999)).toBe("999 B");
    expect(bytes(-1)).toBe("—");
  });
  it("says elapsed time in seconds, then minutes", () => {
    expect(elapsed(4100)).toBe("4.1 s");
    expect(elapsed(125_000)).toBe("2 m 05 s");
  });
  it("says how long ago", () => {
    expect(ago(0, 10_000)).toBe("now");
    expect(ago(0, 8 * 60_000)).toBe("8m ago");
    expect(ago(0, 3 * 3_600_000)).toBe("3h ago");
    expect(ago(0, 72 * 3_600_000)).toBe("3d ago");
  });
  it("holds a percent to its range", () => {
    expect(percent(0.4)).toBe(40);
    expect(percent(2)).toBe(100);
    expect(percent(-1)).toBe(0);
  });
  it("cuts a long id in the middle and leaves a short one whole", () => {
    expect(middle("r-8c21")).toBe("r-8c21");
    const id = "run-20261121-0815-abcdef0123456789";
    const shown = middle(id, 14);
    expect(shown).toHaveLength(14);
    expect(shown.startsWith(id.slice(0, 7))).toBe(true);
    expect(shown.endsWith(id.slice(-6))).toBe(true);
  });
});

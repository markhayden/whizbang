// What the unit tests share: a component rendered into happy-dom's document, read back as
// text and attributes, never as pixels (plan m5, task U1, "Keeping tests lean").
import { render as preactRender, type VNode } from "preact";
import { act } from "preact/test-utils";

/** Text a run could hand the page that would run if it were put in as markup. */
export const HOSTILE = `<img src=x onerror="window.__ran=1">`;

export interface Rendered {
  root: HTMLElement;
  text(): string;
  get<T extends Element = HTMLElement>(selector: string): T;
  all<T extends Element = HTMLElement>(selector: string): T[];
  rerender(next: VNode): void;
  unmount(): void;
}

export function render(node: VNode): Rendered {
  const root = document.createElement("div");
  document.body.appendChild(root);
  act(() => preactRender(node, root));
  return {
    root,
    text: () => (root.textContent ?? "").replace(/\s+/g, " ").trim(),
    get: <T extends Element>(selector: string) => {
      const found = root.querySelector<T>(selector);
      if (!found) throw new Error(`nothing matches ${selector} in a root whose text is: ${root.textContent ?? ""}`);
      return found;
    },
    all: <T extends Element>(selector: string) => Array.from(root.querySelectorAll<T>(selector)),
    rerender: (next) => act(() => preactRender(next, root)),
    unmount: () => {
      act(() => preactRender(null, root));
      root.remove();
    },
  };
}

/** The hostile text is shown as those characters and made no element. */
export function expectShownAsText(rendered: Rendered): void {
  if (!rendered.text().includes(HOSTILE)) throw new Error(`the text was not shown as characters: ${rendered.text()}`);
  if (rendered.root.querySelector("img[onerror]")) throw new Error("the text was put in as markup");
  if ((window as unknown as { __ran?: number }).__ran) throw new Error("the text ran");
}

export function click(element: Element): void {
  act(() => {
    element.dispatchEvent(new MouseEvent("click", { bubbles: true }));
  });
}

export function input(element: HTMLInputElement, value: string): void {
  act(() => {
    element.value = value;
    element.dispatchEvent(new Event("input", { bubbles: true }));
  });
}

// The one wrapper that makes markup (plan m5, task W3): raw HTML is escaped, only http:,
// https:, fragments and relative addresses are kept, and the frame runs nothing and has
// an origin of its own.
import { describe, expect, it } from "vitest";
import { IsolatedFrame, picturesOf, recordDocument, renderMarkdown, safeHref } from "./isolated";
import { render } from "./test/render";

/** Parses HTML the wrapper made into a document of its own, as the frame would. */
function parsed(html: string): Document {
  return new DOMParser().parseFromString(`<!doctype html><body>${html}</body>`, "text/html");
}

describe("the markdown wrapper", () => {
  it("shows <img onerror> as its characters", () => {
    const html = renderMarkdown(`A reason: <img src=x onerror="window.__ran=1"> end`);
    const doc = parsed(html);
    expect(doc.querySelectorAll("img")).toHaveLength(0);
    expect(doc.body.textContent).toContain(`<img src=x onerror="window.__ran=1">`);
  });

  it("shows <script> as its characters, inline and as a block", () => {
    const html = renderMarkdown(`<script>window.__ran=1</script>\n\nText <script>alert(2)</script>`);
    const doc = parsed(html);
    expect(doc.querySelectorAll("script")).toHaveLength(0);
    expect(doc.body.textContent).toContain("<script>window.__ran=1</script>");
    expect(doc.body.textContent).toContain("<script>alert(2)</script>");
  });

  it("drops a javascript: link and keeps its words", () => {
    for (const href of ["javascript:alert(1)", "JaVaScRiPt:alert(1)", " javascript:alert(1)", "java\tscript:alert(1)", "data:text/html,<b>x</b>", "vbscript:x", "//evil.example/x"]) {
      const doc = parsed(renderMarkdown(`[x](${href})`));
      expect(doc.querySelectorAll("a"), href).toHaveLength(0);
      expect(doc.body.textContent, href).toContain("x");
    }
  });

  it("keeps http, https, fragment and relative links", () => {
    const doc = parsed(renderMarkdown("[a](https://example.org/a) [b](http://example.org/b) [c](#limits) [d](view.md)"));
    expect(Array.from(doc.querySelectorAll("a")).map((a) => a.getAttribute("href"))).toEqual(["https://example.org/a", "http://example.org/b", "#limits", "view.md"]);
  });

  it("refuses /\\x, javascript\\: and an entity that spells javascript, in a link", () => {
    for (const href of ["/\\x", "javascript\\:alert(1)", "&#106;avascript:alert(1)"]) {
      const doc = parsed(renderMarkdown(`[x](${href})`));
      const links = Array.from(doc.querySelectorAll("a")).map((a) => a.getAttribute("href") ?? "");
      expect(links, href).toEqual([]);
      expect(doc.body.textContent, href).toContain("x");
    }
  });

  it("shows an absolute picture, https too, as its words", () => {
    const doc = parsed(renderMarkdown("![Far](https://x.example/a.png) ![Near](http://x.example/b.png)"));
    expect(doc.querySelectorAll("img")).toHaveLength(0);
    expect(doc.body.textContent).toContain("Far");
  });

  it("shows a relative picture only from the data address the page made of it", () => {
    const made = "data:image/png;base64,iVBORw0KGgo=";
    const markdown = "![The scene](view/scene.png) ![Missing](view/none.png) ![Bad](javascript:alert(1)) ![Data](data:image/png;base64,AAAA)";
    expect(picturesOf(markdown)).toEqual(["view/scene.png", "view/none.png"]);
    const doc = parsed(renderMarkdown(markdown, new Map([["view/scene.png", made], ["view/none.png", "javascript:alert(1)"]])));
    const images = Array.from(doc.querySelectorAll("img"));
    expect(images.map((img) => img.getAttribute("src"))).toEqual([made]);
    expect(images[0]!.getAttribute("alt")).toBe("The scene");
    expect(doc.body.textContent).toContain("Missing");
    expect(doc.body.textContent).toContain("Bad");
  });

  it("refuses addresses by their scheme", () => {
    expect(safeHref("https://x.example")).toBe("https://x.example");
    expect(safeHref("a/b.png")).toBe("a/b.png");
    expect(safeHref("#x")).toBe("#x");
    expect(safeHref("ftp://x.example/a")).toBeNull();
    expect(safeHref("javascript:alert(1)")).toBeNull();
    expect(safeHref("data:image/png;base64,AAAA")).toBeNull();
    expect(safeHref("//x.example/a.png")).toBeNull();
    for (const href of ["/\\x", "\\/x", "\\\\x", "javascript\\:alert(1)", "&#106;avascript:alert(1)", "a:b"]) {
      expect(safeHref(href), href).toBeNull();
    }
  });

  it("makes a record a document of its own policy", () => {
    const html = recordDocument("view", "# view\n\nWords <b>bold</b>", new Map());
    expect(html).toContain(`<meta http-equiv="Content-Security-Policy" content="default-src 'none'; img-src data:; style-src 'unsafe-inline'">`);
    expect(html).toContain("&lt;b&gt;bold&lt;/b&gt;");
  });
});

describe("the isolated frame", () => {
  it("is sandboxed with nothing allowed: no scripts, no origin of the page's", () => {
    const view = render(<IsolatedFrame html="<p>x</p>" title="The report" />);
    const frame = view.get<HTMLIFrameElement>("iframe");
    expect(frame.getAttribute("sandbox")).toBe("");
    // The document is given inline, by the frame's one attribute that holds it.
    expect(Array.from(frame.attributes).filter((a) => a.value === "<p>x</p>")).toHaveLength(1);
    expect(frame.getAttribute("src")).toBeNull();
    expect(frame.getAttribute("title")).toBe("The report");
    view.unmount();
  });
});

// The one place the interface makes markup and shows it (plan m5, task W3, items 4 and 7):
// a tool's record turned from Markdown into HTML by `marked`, and the isolated frame that
// shows such a document, or a run's report. `markup.test.ts` allows this file, and no
// other, to write markup.
//
// - Raw HTML in the Markdown is shown as its characters: the `html` renderer escapes it.
// - A link or an image keeps its address only when it is http:, https:, a `#` fragment or
//   relative; anything else (javascript:, data:, vbscript:, a `//` address) is dropped,
//   and the words are kept. A relative image is shown only from a `data:` address the
//   page made of its bytes (fetched through the API with the token), never fetched by the
//   frame.
// - The frame is `<iframe sandbox="">`: no `allow-scripts` and no `allow-same-origin`, so
//   its document has an origin of its own, runs nothing, and can reach neither the page,
//   its storage (where the launch token is) nor the API (SPEC.md section 10).
import { Marked, type Tokens } from "marked";

/** The policy a record's document carries of its own, as a report's does. */
export const DOCUMENT_POLICY = "default-src 'none'; img-src data:; style-src 'unsafe-inline'";

/** Text as HTML: the characters that start markup or end an attribute, escaped. */
export function escapeHtml(text: string): string {
  return text.replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c] ?? c);
}

/** The schemes an address may name. */
const SCHEMES = new Set(["http:", "https:"]);

/**
 * The address as given when it is http:, https:, a fragment or relative, else null. An
 * address with a control character or a space in its scheme, or one that starts `//`
 * (another host, whatever the scheme), is refused.
 */
export function safeHref(href: string | null | undefined): string | null {
  if (typeof href !== "string") return null;
  const given = href.trim();
  if (given === "" || /[\u0000-\u001f\u007f]/.test(given)) return null;
  if (given.startsWith("#")) return given;
  if (given.startsWith("//") || given.startsWith("\\")) return null;
  const scheme = /^([^/?#]*?):/.exec(given);
  if (scheme) return SCHEMES.has(`${scheme[1]!.toLowerCase()}:`) ? given : null;
  return given;
}

/** Whether an address is relative: no scheme, no host, no fragment alone. */
function relative(href: string): boolean {
  return !href.startsWith("#") && !/^[a-z][a-z0-9+.-]*:/i.test(href);
}

/** A `data:` address of a picture the page made, and nothing else. */
const PICTURE = /^data:image\/(png|jpeg|webp|gif);base64,[A-Za-z0-9+/=]+$/;

/** The relative addresses of the pictures a Markdown text shows, each once, in order. */
export function picturesOf(markdown: string): string[] {
  const found: string[] = [];
  const marked = new Marked({ async: false, gfm: true });
  marked.walkTokens(marked.lexer(markdown), (token) => {
    if (token.type !== "image") return;
    const href = safeHref((token as Tokens.Image).href);
    if (href && relative(href) && !found.includes(href)) found.push(href);
  });
  return found;
}

/**
 * The Markdown as HTML, held to the rules above. `pictures` maps a relative image's
 * address to the `data:` address the page made of it; one it does not hold, or one that
 * is not such an address, is shown as its words.
 */
export function renderMarkdown(markdown: string, pictures: ReadonlyMap<string, string> = new Map()): string {
  const marked = new Marked({
    async: false,
    gfm: true,
    renderer: {
      html({ text }) {
        return escapeHtml(text);
      },
      link({ href, title, tokens }) {
        const inner = this.parser.parseInline(tokens);
        const safe = safeHref(href);
        if (safe === null) return inner;
        const titled = title ? ` title="${escapeHtml(title)}"` : "";
        return `<a href="${escapeHtml(safe)}"${titled} rel="noreferrer">${inner}</a>`;
      },
      image({ href, title, text }) {
        const safe = safeHref(href);
        const source = safe === null ? null : relative(safe) ? (pictures.get(safe) ?? null) : safe;
        if (source === null || (relative(safe ?? "") && !PICTURE.test(source))) return escapeHtml(text);
        const titled = title ? ` title="${escapeHtml(title)}"` : "";
        return `<img src="${escapeHtml(source)}" alt="${escapeHtml(text)}"${titled}>`;
      },
    },
  });
  return marked.parse(markdown) as string;
}

/** The record's style: a dark page by its colour scheme, so it names no colour of its own. */
const RECORD_STYLE = `:root{color-scheme:dark}body{margin:0;padding:24px 28px;
font:14px/1.6 -apple-system,system-ui,sans-serif;max-width:920px}h1{font-size:24px}h2{font-size:18px;margin-top:28px}
code,pre{font-family:ui-monospace,monospace;font-size:12px}pre{padding:12px;border:1px solid;
border-radius:6px;overflow:auto;white-space:pre-wrap}table{border-collapse:collapse}td,th{border:1px solid;
padding:4px 8px;text-align:left;vertical-align:top}img{max-width:100%;height:auto;image-rendering:pixelated}`;

/** A tool's record as one document of its own: its policy, its style, its HTML. */
export function recordDocument(title: string, markdown: string, pictures: ReadonlyMap<string, string>): string {
  return (
    `<!doctype html><html lang="en"><head><meta charset="utf-8">` +
    `<meta http-equiv="Content-Security-Policy" content="${DOCUMENT_POLICY}">` +
    `<title>${escapeHtml(title)}</title><style>${RECORD_STYLE}</style></head><body>` +
    renderMarkdown(markdown, pictures) +
    `</body></html>`
  );
}

export interface IsolatedFrameProps {
  /** The document: a report the host made, or a record made above. */
  html: string;
  /** What the frame holds, for a reader of the page. */
  title: string;
}

/** A document shown in a frame of its own origin that runs nothing. */
export function IsolatedFrame(props: IsolatedFrameProps) {
  return <iframe class="isolated-frame" sandbox="" referrerpolicy="no-referrer" title={props.title} srcdoc={props.html} />;
}

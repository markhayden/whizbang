// What History, a report and a record ask of the API (plan m5, task W3): this screen's own
// calls, kept beside it, through the client's token.
import type { Client, RunOutput } from "../../api";

/** One run in history, as `GET /api/runs` lists it. */
export interface HistoryRun {
  run: string;
  action: string;
  kit?: string | null;
  title?: string | null;
  words?: string[];
  pictures?: string[];
  result: "pass" | "fail" | "unsure" | "done" | "cancelled" | "error" | "interrupted";
  models?: string[];
  time_ms?: number | null;
  cost?: number;
  at_ms?: number | null;
  thumb?: string | null;
  /** What the run made, by port, type and identity. */
  outputs?: RunOutput[];
}

/** The figures of `runs stats`, as `GET /api/stats` answers them. */
export interface Figures {
  runs: number;
  without_record?: number;
  ended?: { completed: number; error: number; invalid: number; cancelled: number; interrupted: number };
  verdicts?: { pass: number; fail: number; unsure: number };
  retries?: number;
  repairs?: number;
  loops?: { runs: number; attempts: number; retries: number } | null;
  time?: { median_ms: number; p95_ms: number } | null;
  settled_micros?: number;
  unresolved_micros?: number;
  cache_hits?: number;
  text?: string;
}

/** The fields of the filter the address may carry: `runs stats`'s and the page's. */
export const FILTER = ["kit", "task", "result", "words", "action", "model", "backend", "use_case", "matrix", "since", "until", "replayed"];

/** The results a run is listed under, in the order the filter offers them. */
export const RESULTS = ["pass", "fail", "unsure", "done", "cancelled", "error", "interrupted"] as const;

/** How many runs one page of history lists. */
export const PAGE = 50;

/** The filter of an address, as the API takes it: only its known fields, none empty. */
export function filterOf(query: URLSearchParams): URLSearchParams {
  const kept = new URLSearchParams();
  for (const name of FILTER) {
    const value = query.get(name);
    if (value) kept.set(name, value);
  }
  return kept;
}

/** One page of history under the filter, after the run `before` names. */
export async function listRuns(client: Client, query: URLSearchParams, before?: string, limit = PAGE): Promise<{ runs: HistoryRun[]; more: boolean }> {
  const asked = filterOf(query);
  asked.set("limit", String(limit));
  if (before) asked.set("before", before);
  const answer = await client.request<{ runs?: HistoryRun[]; more?: boolean } | undefined>("GET", `/api/runs?${asked.toString()}`);
  return { runs: Array.isArray(answer?.runs) ? answer.runs : [], more: answer?.more === true };
}

/**
 * One page of the runs that made an output of a type (`output=<type>`), newest first,
 * after the run `before` names: what the binding picker offers a port of that type, from
 * the store, so a reload or a restart keeps it.
 */
export async function runsMaking(client: Client, type: string, before?: string, limit = 20): Promise<{ runs: HistoryRun[]; more: boolean }> {
  const asked = new URLSearchParams({ output: type, limit: String(limit) });
  if (before) asked.set("before", before);
  const answer = await client.request<{ runs?: HistoryRun[]; more?: boolean } | undefined>("GET", `/api/runs?${asked.toString()}`);
  return { runs: Array.isArray(answer?.runs) ? answer.runs : [], more: answer?.more === true };
}

/** The figures of `runs stats` under the filter. */
export function figuresOf(client: Client, query: URLSearchParams): Promise<Figures> {
  const asked = filterOf(query).toString();
  return Promise.resolve(client.request<Figures>("GET", asked ? `/api/stats?${asked}` : "/api/stats"));
}

/** A run's report, the document the host made. */
export async function reportOf(client: Client, run: string): Promise<string> {
  return (await client.bytes(`/api/runs/${encodeURIComponent(run)}/report`)).text();
}

/** The task a report names in its head, for the name of its download. */
export function taskOfReport(html: string): string | null {
  return /<meta name="task" content="([a-z][a-z0-9_-]{0,63})">/.exec(html)?.[1] ?? null;
}

/** Saves bytes as a download of this name: the browser saves it, nothing is written here. */
export function save(blob: Blob, name: string): void {
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = name;
  link.rel = "noreferrer";
  document.body.appendChild(link);
  link.click();
  link.remove();
  setTimeout(() => URL.revokeObjectURL(url), 60_000);
}

/** A record's picture address, relative to the record: `<name>/<file>.png`. */
const RECORD_PICTURE = /^[a-z][a-z0-9_-]{0,63}\/[A-Za-z0-9_.-]{1,128}\.png$/;

/** Bytes as a `data:` address. */
function dataUrl(blob: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(String(reader.result));
    reader.onerror = () => reject(reader.error ?? new Error("the picture could not be read"));
    reader.readAsDataURL(blob);
  });
}

/**
 * A record's Markdown and its pictures, each fetched through the API with the token and
 * made a `data:` address, two at a time: a frame with no origin of its own could neither
 * fetch them nor send the token.
 */
export async function recordOf(client: Client, name: string, wanted: (markdown: string) => string[]): Promise<{ markdown: string; pictures: Map<string, string> }> {
  const markdown = await (await client.bytes(`/api/records/${encodeURIComponent(name)}`)).text();
  const asked = wanted(markdown).filter((href) => RECORD_PICTURE.test(href));
  const pictures = new Map<string, string>();
  let next = 0;
  const worker = async () => {
    while (next < asked.length) {
      const href = asked[next++]!;
      try {
        const blob = await client.bytes(`/api/records/${href}`);
        pictures.set(href, await dataUrl(blob.type === "image/png" ? blob : new Blob([blob], { type: "image/png" })));
      } catch {
        // A picture that cannot be had is shown as its words.
      }
    }
  };
  await Promise.all([worker(), worker()]);
  return { markdown, pictures };
}

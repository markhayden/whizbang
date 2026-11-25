// Saved-run comparisons, saved examples and Home's library (plan m7, task U1): the page's
// calls of the host's routes, made through the client's one request (which carries the
// token), the baseline pinned in the browser's storage, and the pure helpers each screen
// lays out. Every text in an answer was escaped by the host; the page puts it in as text.
import { IDENTITY, type Client, type Picture } from "./api";
import type { Draft, Store } from "./drafts";
import { middle } from "./format";

/** One side of a comparison: a run, how it ended, its time, and its family's cost. */
export interface CompareSide {
  run: string;
  action: string;
  title?: string | null;
  outcome: string;
  verdict?: { word: string; why: string } | null;
  start_ms?: number | null;
  end_ms?: number;
  time_ms?: number | null;
  inputs: Array<{ port: string; type: string; id: string }>;
  outputs: Array<{ port: string; type: string; id: string }>;
  cost: { lines: Array<{ capability: string; backend: string; model: string; requests: number; cost: number; free: boolean }>; total: number; unresolved?: number };
  models: string[];
  /** How many runs the family holds, and whether every one was read. */
  runs: number;
  whole: boolean;
}

/** One row of what differs: its section, its name, the two values, whether they differ. */
export interface CompareRow {
  section: "asked" | "resolved" | "settings" | "inputs" | "outputs";
  name: string;
  baseline: string | null;
  attempt: string | null;
  differs: boolean;
}

/** What `GET /api/compare/{baseline}/{attempt}` answers. */
export interface Compared {
  action: string;
  baseline: CompareSide;
  attempt: CompareSide;
  rows: CompareRow[];
}

export function fetchCompare(client: Client, baseline: string, attempt: string): Promise<Compared> {
  return client.request<Compared>("GET", `/api/compare/${encodeURIComponent(baseline)}/${encodeURIComponent(attempt)}`);
}

/** The rows a comparison shows: those that differ, or every one. */
export function rowsShown(rows: CompareRow[], all: boolean): CompareRow[] {
  return all ? rows : rows.filter((row) => row.differs);
}

/** A section's heading, as the design says it. */
export const SECTIONS: Array<{ section: CompareRow["section"]; title: string }> = [
  { section: "asked", title: "Asked" },
  { section: "resolved", title: "Resolved" },
  { section: "settings", title: "Settings" },
  { section: "inputs", title: "Inputs" },
  { section: "outputs", title: "Outputs" },
];

/** The picture a side shows: its first image made, else its first image given. */
export function pictureOf(side: CompareSide): string | null {
  return side.outputs.find((one) => one.type === "image")?.id ?? side.inputs.find((one) => one.type === "image")?.id ?? null;
}

/** The baseline the page has pinned, in the browser's storage: a run and its task. */
export interface Pin {
  run: string;
  action: string;
}

export const PIN_KEY = "whizbang.baseline";

export function readPin(store: Store | null): Pin | null {
  if (!store) return null;
  try {
    const raw = store.getItem(PIN_KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as Partial<Pin>;
    return typeof parsed.run === "string" && typeof parsed.action === "string" ? { run: parsed.run, action: parsed.action } : null;
  } catch {
    return null;
  }
}

export function savePin(store: Store | null, pin: Pin | null): void {
  if (!store) return;
  try {
    if (pin) store.setItem(PIN_KEY, JSON.stringify(pin));
    else store.setItem(PIN_KEY, "");
  } catch {
    // Full or refused: the address still holds the pin.
  }
}

/** An example as `GET /api/examples` lists it. */
export interface Example {
  id: string;
  name: string;
  notes: string;
  task: string;
  title?: string | null;
  kit?: string | null;
  /** Whether this build still has its task: one that does not is read and deleted, not opened. */
  known: boolean;
  run: string;
  at_ms: number;
  picture?: string | null;
  inputs: Array<{ port: string; type: string; id: string }>;
  backends: Record<string, string>;
  /** The name and notes as given, unescaped: what a rename starts from and sends back. */
  given?: { name: string; notes: string };
}

export const EXAMPLES_LIMIT = 1000;
export const EXAMPLE_NAME_LIMIT = 80;
export const EXAMPLE_NOTES_LIMIT = 2000;

export async function listExamples(client: Client): Promise<Example[]> {
  const answer = await client.request<{ examples?: Example[] } | undefined>("GET", "/api/examples");
  return Array.isArray(answer?.examples) ? answer.examples : [];
}

export function saveExample(client: Client, run: string, name: string, notes: string): Promise<Example> {
  return client.request<Example>("POST", "/api/examples", { run, name, notes });
}

export function renameExample(client: Client, id: string, name: string): Promise<Example> {
  return client.request<Example>("PATCH", `/api/examples/${encodeURIComponent(id)}`, { name });
}

export function deleteExample(client: Client, id: string): Promise<{ id: string; deleted: boolean }> {
  return client.request("DELETE", `/api/examples/${encodeURIComponent(id)}`);
}

/** A choice the example made that this config does not offer now, with the reason's kind. */
export interface FellBack {
  argument: string;
  value: string;
  capability: string;
  kind: string;
  why?: string | null;
}

/** What `POST /api/examples/{id}/open` answers: the draft, and what fell back. */
export interface OpenedExample {
  task: string;
  kit: string | null;
  title: string;
  values: Record<string, unknown>;
  example: { id: string; name: string; notes: string; run: string };
  fell_back: FellBack[];
}

export function openExample(client: Client, id: string): Promise<OpenedExample> {
  return client.request<OpenedExample>("POST", `/api/examples/${encodeURIComponent(id)}/open`);
}

/** Where a draft opened from an example came from, as the workspace keeps it. */
export interface FromExample {
  id: string;
  name: string;
  fellBack: FellBack[];
}

/** The draft an example opens, as the workspace keeps drafts. */
export function draftOfExample(opened: OpenedExample, now: number): Draft & { example: FromExample } {
  const pictures: Draft["pictures"] = {};
  for (const value of Object.values(opened.values)) {
    for (const one of Array.isArray(value) ? value : [value]) {
      if (typeof one === "string" && /^sha256:[0-9a-f]{64}$/.test(one)) pictures[one] = { id: one, name: `${opened.example.name} · ${middle(one, 14)}` };
    }
  }
  return {
    kit: opened.kit ?? "",
    task: opened.task,
    values: { ...opened.values },
    pictures,
    at: now,
    example: { id: opened.example.id, name: opened.example.name, fellBack: opened.fell_back },
  };
}

/** A choice that fell back, in words: what the example asked, why it is not here, what is used. */
export function fellBackWords(one: FellBack, role: string): string {
  const why =
    one.kind === "not_in_config"
      ? "this config names no such backend now"
      : one.kind === "no_key"
        ? "its key is not set here"
        : one.kind === "not_installed"
          ? "it is not installed here"
          : one.kind === "refused"
            ? "this task refuses it now"
            : "it is not ready here";
  return `The example asked for the ${role} ${one.value}; ${why}, so this draft uses the config's default.`;
}

/** Which pictures of the library Home shows: its chips. */
export type Show = "all" | "named" | "inputs" | "imported";

export const SHOWS: Array<{ value: Show; label: string }> = [
  { value: "all", label: "All" },
  { value: "named", label: "Named" },
  { value: "inputs", label: "Inputs of runs" },
  { value: "imported", label: "Imported" },
];

/** A picture of the library, as `GET /api/library` lists it. */
export interface LibraryPicture {
  id: string;
  /** The name a person gave it in the playground, or none. */
  name: string | null;
  runs: number;
  imported: boolean;
  first_ms?: number | null;
  last_ms?: number;
}

export interface LibraryPage {
  pictures: LibraryPicture[];
  more: boolean;
  counts: Record<Show, number>;
}

/** The most pictures Home asks for at once. */
export const LIBRARY_PAGE = 24;

export async function listLibrary(client: Client, show: Show, after?: string, limit = LIBRARY_PAGE): Promise<LibraryPage> {
  const from = after ? `&after=${encodeURIComponent(after)}` : "";
  const answer = await client.request<Partial<LibraryPage> | undefined>("GET", `/api/library?show=${show}&limit=${limit}${from}`);
  return {
    pictures: Array.isArray(answer?.pictures) ? answer.pictures : [],
    more: answer?.more === true,
    counts: { all: 0, named: 0, inputs: 0, imported: 0, ...(answer?.counts ?? {}) },
  };
}

/** One picture's card: its size, its bytes, and the runs that took it. */
export interface LibraryCard extends LibraryPicture {
  width?: number | null;
  height?: number | null;
  bytes?: number;
  runs_list: Array<{ run: string; at_ms: number; action: string; title?: string | null; outcome: string }>;
}

export function fetchCard(client: Client, id: string): Promise<LibraryCard> {
  // An identity is sent as it is: its colon is a character a path holds, and the host
  // reads the path as sent.
  const path = IDENTITY.test(id) ? id : encodeURIComponent(id);
  return client.request<LibraryCard>("GET", `/api/library/${path}`);
}

/** A picture's name as the library shows it: the name a person gave, or its identity, shortened. */
export function shownName(picture: Pick<LibraryPicture, "id" | "name">): string {
  return picture.name ?? middle(picture.id, 20);
}

/** What a tile says under its name: its count of runs, or that it was imported. */
export function tileLine(picture: LibraryPicture): string {
  if (picture.runs > 0) return picture.runs === 1 ? "1 run" : `${picture.runs} runs`;
  return picture.imported ? "imported · not run yet" : "imported";
}

/** A library picture as the rest of the page takes a picture. */
export function asPicture(picture: LibraryPicture | LibraryCard): Picture {
  const sized = picture as LibraryCard;
  return { id: picture.id, name: shownName(picture), width: sized.width ?? undefined, height: sized.height ?? undefined };
}

/** A moment as the card says it: "24 Nov 14:02". */
export function momentWords(ms: number): string {
  const at = new Date(ms);
  const months = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
  const two = (n: number) => String(n).padStart(2, "0");
  return `${at.getDate()} ${months[at.getMonth()]} ${two(at.getHours())}:${two(at.getMinutes())}`;
}

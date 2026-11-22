// Drafts (plan m5, "What the design shows", Home's Continue): a task with its inputs given
// and not yet run, kept in the browser's own storage for the host's fixed address. A
// convenience: storage that is missing, full or refused loses drafts and nothing else, so
// every read and write is guarded.
import type { Picture } from "./api";
import type { Values } from "./form";

export const DRAFTS_KEY = "whizbang.drafts";
/** The most drafts kept: the oldest go first. */
export const DRAFTS_LIMIT = 12;

export interface Draft {
  kit: string;
  task: string;
  values: Values;
  /** What the page knows of the pictures the values name, to show them again. */
  pictures: Record<string, Picture>;
  at: number;
  /** The result this draft was sent from (plan m5, task W2, item 3). */
  sent?: Sent;
  /** A check run again takes the job of the run it came from (task W2, item 4). */
  jobOf?: string;
}

/** Where a draft that Send to opened came from: the run, its kit and task, and when. */
export interface Sent {
  run: string;
  /** The sending run's kit and task, by the names a person reads. */
  kit: string;
  title: string;
  at: number;
  /** The fields it filled, tagged FROM <KIT>. */
  fields: string[];
  /** The run whose job the check takes (the loop's, from a removal's phrase). */
  jobOf?: string;
}

export interface Store {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
}

/** The page's local storage, or none where it cannot be reached. */
export function localStore(): Store | null {
  try {
    return globalThis.localStorage ?? null;
  } catch {
    return null;
  }
}

function isDraft(value: unknown): value is Draft {
  if (typeof value !== "object" || value === null) return false;
  const draft = value as Record<string, unknown>;
  return typeof draft.kit === "string" && typeof draft.task === "string" && typeof draft.values === "object" && draft.values !== null && typeof draft.at === "number";
}

/** Every draft kept, newest first; nothing when storage cannot be read or holds no drafts. */
export function readDrafts(store: Store | null): Draft[] {
  if (!store) return [];
  try {
    const raw = store.getItem(DRAFTS_KEY);
    if (!raw) return [];
    const parsed: unknown = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    return parsed
      .filter(isDraft)
      .map((draft) => ({ ...draft, pictures: typeof draft.pictures === "object" && draft.pictures !== null ? draft.pictures : {} }))
      .sort((a, b) => b.at - a.at);
  } catch {
    return [];
  }
}

function write(store: Store | null, drafts: Draft[]): void {
  if (!store) return;
  try {
    store.setItem(DRAFTS_KEY, JSON.stringify(drafts.slice(0, DRAFTS_LIMIT)));
  } catch {
    // Full or refused: the draft is not kept, and nothing else is lost.
  }
}

/** The draft of one task, if one is kept. */
export function draftOf(store: Store | null, task: string): Draft | undefined {
  return readDrafts(store).find((draft) => draft.task === task);
}

/** Keeps the draft of its task in place of an older one. */
export function saveDraft(store: Store | null, draft: Draft): void {
  write(store, [draft, ...readDrafts(store).filter((kept) => kept.task !== draft.task)]);
}

/** Forgets the draft of a task: it was run. */
export function dropDraft(store: Store | null, task: string): void {
  const kept = readDrafts(store);
  if (kept.some((draft) => draft.task === task)) write(store, kept.filter((draft) => draft.task !== task));
}

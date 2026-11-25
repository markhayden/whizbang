// Bundles, guided chain replay and the replay of a run (plan m7, task U2): the page's calls
// of the host's routes, made through the client's one request (which carries the token),
// a chain's place kept in the browser's storage, and the pure helpers the screens lay out.
// Every text in an answer was escaped by the host; the page puts it in as text.
import type { Client } from "./api";
import type { Draft, Store } from "./drafts";
import { middle } from "./format";

/** An input of a bundle's step: the identity it took, or the earlier step's output. */
export interface StepInput {
  port: string;
  place: number;
  artifact: string | null;
  from: { step: number; port: string; place: number } | null;
}

/** A step of a bundle: its run, its task, and whether this build replays it, and why not. */
export interface BundleStep {
  step: number;
  run: string;
  action: string | null;
  title?: string | null;
  version: string | null;
  runs: number;
  replayable: boolean;
  why: string | null;
  inputs: StepInput[];
}

/** What a bundle holds and withholds, as the host shows one. */
export interface BundleSummary {
  digest: string;
  whizbang?: string;
  steps: BundleStep[];
  holds: { records: number; files: number };
  withheld: Array<{ run: string; what: string; why: string }>;
  /** After an import: whether it was new to the store. */
  imported?: boolean;
  bytes?: number;
  unreadable?: boolean;
}

/** What one run's bundle would hold, before it is written. */
export interface BundleAbout {
  run: string;
  action: string | null;
  about_bytes: number;
  runs: number;
  inputs: number;
  pictures: number;
  outputs: number;
  masks: number;
}

export function bundleAbout(client: Client, run: string): Promise<BundleAbout> {
  return client.request<BundleAbout>("GET", `/api/runs/${encodeURIComponent(run)}/bundle/about`);
}

export function bundleRun(client: Client, run: string): Promise<Blob> {
  return client.bytes(`/api/runs/${encodeURIComponent(run)}/bundle`);
}

/** A chain's steps and links, or the refusal, without writing it. */
export function checkChain(client: Client, runs: string[]): Promise<BundleSummary> {
  return client.request<BundleSummary>("POST", "/api/bundles/chain", { runs, check: true });
}

export function bundleChain(client: Client, runs: string[]): Promise<Blob> {
  return client.postBytes("/api/bundles/chain", { runs });
}

export function importBundle(client: Client, file: Blob): Promise<BundleSummary> {
  return client.upload<BundleSummary>("/api/bundles", file);
}

export async function listBundles(client: Client): Promise<BundleSummary[]> {
  const answer = await client.request<{ bundles?: BundleSummary[] } | undefined>("GET", "/api/bundles");
  return Array.isArray(answer?.bundles) ? answer.bundles : [];
}

export function showBundle(client: Client, digest: string): Promise<BundleSummary> {
  return client.request<BundleSummary>("GET", `/api/bundles/${digest}`);
}

export function deleteBundle(client: Client, digest: string): Promise<{ digest: string; deleted: boolean }> {
  return client.request("DELETE", `/api/bundles/${digest}`);
}

/** A linked input of a step, bound to the new run's output, beside the one recorded. */
export interface Binding {
  port: string;
  place: number;
  from: { step: number; port: string; place: number };
  run: string;
  now: string;
  recorded: string;
  differs: boolean;
}

/** What opening a step answers: its draft, or where the chain stops. */
export interface OpenedStep {
  task?: string;
  kit?: string | null;
  title?: string;
  values?: Record<string, unknown>;
  step: { index: number; of: number; run: string; action: string; title?: string | null };
  bindings: Binding[];
  stopped: { why: string; run?: string } | null;
  bundle?: string;
}

export function openStep(client: Client, digest: string, step: number, runs: Array<string | null>): Promise<OpenedStep> {
  return client.request<OpenedStep>("POST", `/api/bundles/${digest}/steps/${step}/open`, { runs });
}

/** Starts a replay of a run: offline, or live. */
export function startReplay(client: Client, run: string, live: boolean, requestId: string): Promise<{ run: string; state?: string }> {
  return client.request("POST", `/api/runs/${encodeURIComponent(run)}/replay`, { live, request_id: requestId });
}

/** One difference of a replay from the run it replays, by place. */
export interface Difference {
  place: string;
  what: string;
  original: string;
  replay: string;
}

/** A replay's report, once it has ended. */
export interface ReplayReport {
  replay: string;
  of: string;
  bundle: { digest: string; step: number } | null;
  mode: "offline" | "live";
  report: "reproduced" | "differs" | "diverged" | "unanswerable";
  differences: Difference[];
  more: number;
  stopped: { kind: string; capability: string; request: number; why: string } | null;
  settings: Difference[];
  spent_micros: number;
}

export function fetchReplayReport(client: Client, run: string): Promise<ReplayReport> {
  return client.request<ReplayReport>("GET", `/api/runs/${encodeURIComponent(run)}/replay`);
}

/** A report's headline, as the design says it. */
export function reportTitle(report: ReplayReport): string {
  switch (report.report) {
    case "reproduced":
      return "Reproduced";
    case "differs": {
      const count = report.differences.length + report.more;
      return `Differs in ${count} ${count === 1 ? "place" : "places"}`;
    }
    case "diverged":
      return report.stopped ? `Diverged at ${report.stopped.capability} request ${report.stopped.request}` : "Diverged";
    case "unanswerable":
      return "Can't be answered offline";
  }
}

/** A size in words: "3.1 MB", "about 3.1 MB". */
export function sizeWords(bytes: number, about = false): string {
  const units = ["bytes", "KB", "MB", "GB"];
  let value = bytes;
  let unit = 0;
  while (value >= 1000 && unit < units.length - 1) {
    value /= 1000;
    unit += 1;
  }
  const shown = unit === 0 ? String(value) : value.toFixed(1);
  return `${about ? "about " : ""}${shown} ${units[unit]}`;
}

/** A bundle's name as a person reads it: its first step's task and its digest, shortened. */
export function bundleName(summary: Pick<BundleSummary, "digest" | "steps">): string {
  const first = summary.steps[0];
  const task = first?.title ?? first?.action ?? "bundle";
  return summary.steps.length > 1 ? `A chain of ${summary.steps.length} steps` : `${task} · ${middle(summary.digest, 18)}`;
}

/** The links a chain holds, in words: "1's image → 2's candidate". */
export function linksOf(summary: Pick<BundleSummary, "steps">): string[] {
  const links: string[] = [];
  for (const step of summary.steps) {
    for (const input of step.inputs) {
      if (input.from) links.push(`${input.from.step + 1}'s ${input.from.port} → ${step.step + 1}'s ${input.port}`);
    }
  }
  return links;
}

// A chain's place (plan m7, "Guided chain replay"): which step, which runs, kept in the
// browser's storage for the page's address. Closing the tab loses nothing of a run; a
// chain reopened is read again from its runs and its bundle.

export const CHAINS_KEY = "whizbang.chains";
/** The most chains kept: the oldest go first. */
export const CHAINS_LIMIT = 16;

export interface ChainPlace {
  digest: string;
  steps: number;
  /** The new run of each step, or none yet. */
  runs: Array<string | null>;
  /** The person stopped the chain. */
  stopped: boolean;
  at: number;
}

export function readChains(store: Store | null): ChainPlace[] {
  if (!store) return [];
  try {
    const raw = store.getItem(CHAINS_KEY);
    if (!raw) return [];
    const parsed: unknown = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    return parsed.filter(
      (one): one is ChainPlace =>
        typeof one === "object" && one !== null && typeof (one as ChainPlace).digest === "string" && Array.isArray((one as ChainPlace).runs) && typeof (one as ChainPlace).steps === "number",
    );
  } catch {
    return [];
  }
}

export function chainOf(store: Store | null, digest: string): ChainPlace | null {
  return readChains(store).find((one) => one.digest === digest) ?? null;
}

export function saveChain(store: Store | null, place: ChainPlace): void {
  if (!store) return;
  try {
    const rest = readChains(store).filter((one) => one.digest !== place.digest);
    store.setItem(CHAINS_KEY, JSON.stringify([place, ...rest].slice(0, CHAINS_LIMIT)));
  } catch {
    // Full or refused: the chain's runs are in the store all the same.
  }
}

/** The chain a run was made for, and its step, when the page keeps one that names it. */
export function chainOfRun(store: Store | null, run: string): { place: ChainPlace; step: number } | null {
  for (const place of readChains(store)) {
    const step = place.runs.indexOf(run);
    if (step >= 0) return { place, step };
  }
  return null;
}

/** A chain's place with the run of one step, and the later steps' runs forgotten. */
export function withRun(place: ChainPlace, step: number, run: string): ChainPlace {
  const runs = place.runs.slice(0, step);
  while (runs.length < step) runs.push(null);
  runs.push(run);
  return { ...place, runs, stopped: false, at: Date.now() };
}

/** Each step's state, as the chain's list shows it. */
export type StepState = "ended" | "review" | "waiting" | "stopped" | "blocked";

export function stepStates(place: ChainPlace, blockedAt: number | null): StepState[] {
  const next = place.runs.findIndex((run) => run === null);
  const current = next === -1 ? place.runs.length : next;
  return Array.from({ length: place.steps }, (_, at) => {
    if (place.runs[at]) return "ended";
    if (blockedAt === at) return "blocked";
    if (place.stopped && at >= current) return "stopped";
    return at === current ? "review" : "waiting";
  });
}

/** The draft a chain's step opens, as the workspace keeps drafts. */
export function draftOfStep(opened: OpenedStep, digest: string, now: number): Draft & { chain: NonNullable<Draft["chain"]> } {
  const pictures: Draft["pictures"] = {};
  const linked = new Map(opened.bindings.map((one) => [one.now, one]));
  for (const value of Object.values(opened.values ?? {})) {
    for (const one of Array.isArray(value) ? value : [value]) {
      if (typeof one !== "string" || !/^sha256:[0-9a-f]{64}$/.test(one)) continue;
      const binding = linked.get(one);
      pictures[one] = { id: one, name: binding ? `step ${binding.from.step + 1}'s ${binding.from.port} · ${middle(one, 14)}` : `from the bundle · ${middle(one, 14)}` };
    }
  }
  return {
    kit: opened.kit ?? "",
    task: opened.task ?? opened.step.action,
    values: { ...(opened.values ?? {}) },
    pictures,
    at: now,
    chain: {
      digest,
      step: opened.step.index,
      of: opened.step.of,
      bindings: opened.bindings.map((one) => ({ port: one.port, from: { step: one.from.step, port: one.from.port }, run: one.run, now: one.now, recorded: one.recorded, differs: one.differs })),
    },
  };
}

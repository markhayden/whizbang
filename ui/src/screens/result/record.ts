// The result (plan m5, task W2): what the page asks of a run that ended, and what it makes
// of the answer. The calls are this screen's own routes (`GET /api/runs/{id}/record` and
// `…/command`), made through the client's one request, which carries the token. Pure
// helpers, so each is tested alone (result.test.tsx); the screen only lays them out.
import type { Client, HelloKit } from "../../api";
import type { Mark } from "../../components/Canvas";
import type { CostLine } from "../../components/CostTable";
import { kitTone } from "../../components/kitTone";
import type { SendTarget } from "../../components/SendToMenu";
import type { Verdict } from "../../components/VerdictCard";
import { carriedOf, type Draft, type JobOf, type Sent } from "../../drafts";
import type { Heard } from "../../form";
import { middle } from "../../format";

/** One input or output of a run: its port, its artifact's type and its identity. */
export interface Artifact {
  port: string;
  type: string;
  id: string;
}

/** A place a verdict's finding cites, in fractions of the picture, from 0 to 1. */
export interface Place {
  image: string;
  box?: [number, number, number, number] | null;
  polygon?: Array<[number, number]> | null;
}

/** A draft a result opens in another task (Send to) or in its own (Run again). */
export interface OpenDraft {
  task: string;
  kit: string | null;
  title: string;
  values: Record<string, unknown>;
  /** The fields the result filled, tagged FROM <KIT> in the draft. */
  from: string[];
  /** What the draft gets, in words. */
  line?: string;
  /** The call whose job a check takes, read by the host from the store. */
  job_of?: JobOf;
  /** What Run again's run held that no field shows, sent as it was. */
  kept?: Record<string, unknown>;
}

export interface Attempt {
  attempt: number;
  verdict: Verdict | null;
  unchecked: string | null;
  changed: string | null;
  cost: number;
  /** The picture the attempt's fill made: a diagnostic of the loop's, not its output. */
  picture: string | null;
  diagnostic: boolean;
}

/** What `GET /api/runs/{id}/record` answers. */
export interface RunRecordView {
  run: string;
  parent?: string | null;
  action: string | null;
  title?: string | null;
  outcome: string;
  error?: { kind: string; message: string } | null;
  invalid?: string | null;
  start_ms?: number | null;
  end_ms?: number;
  arguments: Array<{ name: string; value: unknown }>;
  inputs: Artifact[];
  outputs: Artifact[];
  verdict?: { word: string; why: string; reasons: string[]; places: Place[] } | null;
  /** `reason`: why the attempts ended as a word of a closed set, where the record holds one (`run/8`). */
  attempts?: { list: Attempt[]; kept: number | null; ended: string; reason?: string | null } | null;
  cost?: { lines: Array<{ capability: string; backend: string; model: string; requests: number; cost: number; free: boolean }>; total: number; unresolved?: number };
  reserved?: number | null;
  send_to?: OpenDraft[];
  again?: OpenDraft | null;
  /** A replay, or a chain's step: the run it makes again (plan m7, task U2). */
  replayed?: { run: string; bundle: { digest: string; step: number } | null; mode: "offline" | "live" } | null;
}

/** What `GET /api/runs/{id}/command` answers. */
export interface CommandView {
  command: string;
  words: string[];
  config_changed: boolean;
  recorded_config?: string | null;
  config?: string | null;
}

/** The record of a run that ended. */
export function fetchRecord(client: Client, run: string): Promise<RunRecordView> {
  return client.request<RunRecordView>("GET", `/api/runs/${encodeURIComponent(run)}/record`);
}

/** The command line that makes the run's call again. */
export function fetchCommand(client: Client, run: string): Promise<CommandView> {
  return client.request<CommandView>("GET", `/api/runs/${encodeURIComponent(run)}/command`);
}

/** A verdict the card shows, or none for a word it does not know. */
export function verdictOf(word: string | null | undefined): Verdict | null {
  return word === "pass" || word === "fail" || word === "unsure" ? word : null;
}

/** The verdict in words. */
export function verdictTitle(verdict: Verdict): string {
  switch (verdict) {
    case "pass":
      return "Check passed";
    case "fail":
      return "The check failed";
    case "unsure":
      return "Not sure";
  }
}

/** The cost table's lines: each capability with how often it was asked, and its backend. */
export function costLines(cost: RunRecordView["cost"]): CostLine[] {
  const lines = Array.isArray(cost?.lines) ? cost.lines : [];
  return lines.map((line) => ({
    what: line.requests > 1 ? `${line.capability} × ${line.requests}` : line.capability,
    backend: line.free ? `${line.backend} local` : line.backend,
    cost: line.free ? 0 : line.cost,
  }));
}

/** The words of an attempt's outcome. */
export function attemptWords(attempt: Attempt): string {
  if (attempt.verdict) return attempt.verdict;
  return attempt.unchecked ?? "not checked";
}

/** A kit's tone, by its place in the description's kits. */
export function toneOf(kits: HelloKit[], kit: string | null | undefined) {
  return kitTone(Math.max(0, kits.findIndex((one) => one.id === kit)));
}

/** A kit's name, as hello says it. */
export function kitName(kits: HelloKit[], kit: string | null | undefined): string {
  return kits.find((one) => one.id === kit)?.name ?? kit ?? "";
}

/** The Send to menu's targets: each draft a result opens, by its kit's tone. */
export function targetsOf(drafts: OpenDraft[], kits: HelloKit[]): SendTarget[] {
  return drafts.map((draft) => ({
    id: draft.task,
    title: `${kitName(kits, draft.kit)} · ${draft.title}`,
    line: draft.line ?? "",
    tone: toneOf(kits, draft.kit),
  }));
}

/**
 * The draft a result opens, as the workspace keeps drafts: its values, the pictures they
 * name with the words that say whose they are, and, for Send to, where it was sent from.
 */
export function draftFrom(open: OpenDraft, record: RunRecordView, kits: HelloKit[], sent: boolean, now: number): Draft {
  const pictures: Draft["pictures"] = {};
  const named = (id: string): string => {
    if (record.outputs.some((output) => output.id === id)) return `${middle(record.run, 16)} · result`;
    if (record.inputs.some((input) => input.id === id)) return `${middle(record.run, 16)} · its picture`;
    return middle(id, 20);
  };
  for (const value of Object.values(open.values)) {
    for (const one of Array.isArray(value) ? value : [value]) {
      if (typeof one === "string" && /^sha256:[0-9a-f]{64}$/.test(one)) pictures[one] = { id: one, name: named(one) };
    }
  }
  const from: Sent | undefined = sent
    ? {
        run: record.run,
        kit: kitName(kits, (record.action && kits.find((kit) => kit.tasks?.some((task) => task.name === record.action))?.id) || null),
        title: record.title ?? record.action ?? "",
        at: record.end_ms ?? now,
        fields: open.from,
        jobOf: open.job_of,
      }
    : undefined;
  return {
    kit: open.kit ?? "",
    task: open.task,
    values: { ...open.values },
    pictures,
    at: now,
    ...(from ? { sent: from } : {}),
    ...(sent ? {} : carriedOf(open)),
  };
}

/**
 * The places a verdict cites on one picture, as the canvas's flags, in that picture's
 * pixels, each with the words of the reason at its place when there are as many reasons.
 */
export function flagsOf(places: Place[] | undefined, image: string | null, width: number, height: number): Mark[] {
  if (!image || !Array.isArray(places)) return [];
  const marks: Mark[] = [];
  places.forEach((place, index) => {
    if (place.image !== image) return;
    let box = place.box ?? null;
    if (!box && Array.isArray(place.polygon) && place.polygon.length > 0) {
      const xs = place.polygon.map((point) => point[0]);
      const ys = place.polygon.map((point) => point[1]);
      box = [Math.min(...xs), Math.min(...ys), Math.max(...xs), Math.max(...ys)];
    }
    if (!box) return;
    const [left, top, right, bottom] = box;
    marks.push({ kind: "flag", x: left * width, y: top * height, w: (right - left) * width, h: (bottom - top) * height, words: `place ${index + 1}` });
  });
  return marks;
}

/**
 * The runs of the checks a run has made so far, newest last: each step that started a
 * call of one of `checks` and has ended, by the child run its end names (plan m5, task W1,
 * item 6, left to this task's record route: a check's places drawn while a loop runs).
 */
export function checksEnded(heard: Heard, checks: ReadonlySet<string>): string[] {
  const started = new Map<string, string>();
  const runs: string[] = [];
  for (const item of heard) {
    if ("skipped" in item || !item.event) continue;
    const event = item.event;
    if (event.kind === "step_started") started.set(`${event.attempt ?? 0}:${event.step}`, event.action);
    if (event.kind === "step_ended" && event.outcome === "completed") {
      const action = started.get(`${event.attempt ?? 0}:${event.step}`);
      if (action && checks.has(action) && !runs.includes(event.run)) runs.push(event.run);
    }
  }
  return runs;
}

/** The core's words for a step that found nothing (`recipe::runner`): its name and action. */
const FOUND_NOTHING = /the step (\S+) \((\S+)\) found nothing/;

/**
 * A run that ended because a step found nothing, said plainly (presentation only: the
 * record's outcome stays what it is): "Nothing in the picture matched "the thing". Name
 * what to remove more exactly." Null for any other end.
 */
export function nothingFound(record: Pick<RunRecordView, "invalid" | "error" | "arguments">): string | null {
  const said = [record.invalid, record.error?.message].find((words) => typeof words === "string" && FOUND_NOTHING.test(words));
  if (!said) return null;
  const asked = record.arguments.find((one) => ["prompt", "words", "phrase", "query"].includes(one.name) && typeof one.value === "string" && one.value.trim() !== "");
  if (typeof asked?.value === "string") return `Nothing in the picture matched "${asked.value.trim()}". Name what to remove more exactly.`;
  return "Nothing was found in the picture where the task looked. Point at it more exactly, or name it.";
}

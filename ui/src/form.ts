// What the workspace and a running run make of a task's form, a preparation and a run's
// events (plan m5, task W1, items 4 to 6). Pure, so each is tested alone (form.test.ts):
// the screens only lay out what these return.
import type { DrawnShape, HelloBackend, JournalLine, Prepared, Task, TaskField } from "./api";
import type { Choice, ReadinessKind } from "./components/ChoiceList";
import type { Mark } from "./components/Canvas";
import type { PlanNeed } from "./components/PlanBlock";
import type { StepListEntry } from "./components/StepList";
import { middle } from "./format";

/** What a person has given a task, by field name: text, a number, a choice, an id or ids. */
export type Values = Record<string, unknown>;

const READINESS: ReadonlySet<string> = new Set(["ready", "no_key", "not_installed", "out_of_date", "damaged", "not_in_catalogue", "not_for_this_platform", "other"]);

function readiness(kind: string | null | undefined, ready: boolean): ReadinessKind {
  if (ready) return "ready";
  return kind && READINESS.has(kind) && kind !== "ready" ? (kind as ReadinessKind) : "other";
}

/** What hello says of a backend, before any preparation: "checking" when it is not certain. */
function hellosReadiness(backend: HelloBackend, settled: boolean): ReadinessKind {
  if (backend.ready === true) return "ready";
  const said = readiness(backend.missing, false);
  return said === "other" && !settled ? "checking" : said;
}

/** A field's label: the host's, or its name in words. */
export function labelOf(field: TaskField): string {
  if (field.label) return field.label;
  const words = field.name.replace(/_/g, " ");
  return words.charAt(0).toUpperCase() + words.slice(1);
}

/** The fields of a task, as the host gave them: never anything but an array. */
export function fieldsOf(task: Task | undefined): TaskField[] {
  return Array.isArray(task?.fields) ? task.fields : [];
}

/** Whether a value says something: text that is not blank, a list that is not empty. */
export function given(value: unknown): boolean {
  if (value === undefined || value === null) return false;
  if (typeof value === "string") return value.trim() !== "";
  if (Array.isArray(value)) return value.length > 0;
  return true;
}

/** The call's arguments: each field that says something, by its name; nothing else. */
export function argumentsOf(task: Task | undefined, values: Values): Record<string, unknown> {
  const args: Record<string, unknown> = {};
  for (const field of fieldsOf(task)) {
    const value = values[field.name];
    if (!given(value)) continue;
    args[field.name] = typeof value === "string" ? value.trim() : value;
  }
  return args;
}

/** The labels of the required fields that say nothing yet. */
export function unfilled(task: Task | undefined, values: Values): string[] {
  return fieldsOf(task)
    .filter((field) => field.required && !given(values[field.name]))
    .map(labelOf);
}

/**
 * What a field shows when the person has said nothing: the value a choice, a yes-or-no, a
 * small whole number or a backend is shown with. The workspace seeds its values with these
 * ([`seeded`]), so what is shown is what is sent to prepare. None for a field that shows
 * nothing selected (text, a number field, a picture).
 */
export function displayed(field: TaskField, value: unknown, backends: HelloBackend[]): unknown {
  switch (field.kind) {
    case "choice": {
      if (typeof value === "string") return value;
      const choices = Array.isArray(field.choices) ? field.choices : [];
      return typeof field.default === "string" ? field.default : choices[0];
    }
    case "boolean":
      return typeof value === "boolean" ? value : field.default === true;
    case "integer": {
      if (typeof value === "number") return value;
      const { min, max } = field;
      if (typeof min !== "number" || typeof max !== "number" || max - min > 8) return undefined;
      return typeof field.default === "number" ? field.default : min;
    }
    case "backend":
      return typeof value === "string" ? value : (defaultChoice(field, backends) ?? undefined);
    default:
      return value;
  }
}

/** The values with each field that shows something it was not given set to what it shows. */
export function seeded(task: Task | undefined, values: Values, backends: HelloBackend[]): Values {
  const next: Values = { ...values };
  let changed = false;
  for (const field of fieldsOf(task)) {
    if (field.name in values) continue;
    const shown = displayed(field, undefined, backends);
    if (shown === undefined || shown === null) continue;
    next[field.name] = shown;
    changed = true;
  }
  return changed ? next : values;
}

/** The backends hello lists for one capability. */
export function backendsFor(capability: string | null | undefined, backends: HelloBackend[]): HelloBackend[] {
  return backends.filter((backend) => backend.capability === capability);
}

/**
 * The choices of a backend field: from its own preparations when there are any, else
 * hello's readiness, until the first preparation answers. Hello's readiness is read
 * without a probe: a backend it says is ready, or names a reason for (no key, not
 * installed), is shown so; one it says nothing certain of is "checking", never "not
 * ready" before anything has said so: until the first preparation has settled (`settled`),
 * answered or failed, after which it is not ready.
 */
export function choicesFor(field: TaskField, prepared: Prepared | null, backends: HelloBackend[], settled = false): Choice[] {
  const sizes = new Map(backends.map((backend) => [`${backend.capability}:${backend.name}`, backend.size ?? undefined]));
  const own = prepared?.choices?.[field.name];
  if (Array.isArray(own) && own.length > 0) {
    return own.map((choice) => ({
      value: choice.value,
      label: choice.value,
      readiness: readiness(choice.kind, choice.ready),
      size: choice.size ?? sizes.get(`${field.capability}:${choice.value}`),
      why: choice.why ?? undefined,
    }));
  }
  return backendsFor(field.capability, backends).map((backend) => ({
    value: backend.name,
    label: backend.model ? `${backend.name} · ${backend.model}` : backend.name,
    readiness: hellosReadiness(backend, settled),
    size: backend.size ?? undefined,
    why: backend.why,
  }));
}

/** The backend a call uses for a field that names none: the config's default. */
export function defaultChoice(field: TaskField, backends: HelloBackend[]): string | null {
  const of = backendsFor(field.capability, backends);
  return (of.find((backend) => backend.default) ?? of[0])?.name ?? null;
}

/** The plan block's lines from a preparation's needs. */
export function planNeeds(prepared: Prepared | null, backends: HelloBackend[] = []): PlanNeed[] {
  const needs = Array.isArray(prepared?.needs) ? prepared.needs : [];
  // The host names the config's backend the preparation resolved; only an answer that
  // names none falls back to the capability's default.
  const named = (capability: string, backend: string | null | undefined): string => {
    if (backend) return backend;
    const of = backendsFor(capability, backends);
    return (of.find((one) => one.default) ?? of[0])?.name ?? "";
  };
  return needs.map((need) => ({
    capability: need.capability,
    backend: named(need.capability, need.backend),
    model: need.model ?? undefined,
    requests: need.requests,
    each: need.each,
    total: need.bound,
    local: need.local,
    ...(need.ready === false ? { ready: false, why: need.why ?? undefined } : {}),
  }));
}

/** Why Run cannot be pressed now, in words; null when it can. */
export function whyNotRun(task: Task | undefined, values: Values, prepared: Prepared | null, preparing: boolean): string | null {
  const empty = unfilled(task, values);
  if (empty.length > 0) return `Give ${empty.join(", ")}.`;
  if (preparing || !prepared) return "Preparing…";
  if (prepared.refused) return prepared.why ?? "The host refused this call.";
  if (!prepared.ready) {
    const missing = Array.isArray(prepared.missing) ? prepared.missing : [];
    if (missing.length === 0) return prepared.why ?? "This call cannot run now.";
    // A key that is not set is said as the host says it, a sentence of its own: "the
    // filler needs FAL_KEY in your env file".
    return missing.map((one) => (one.kind === "no_key" ? `${one.why.charAt(0).toUpperCase()}${one.why.slice(1)}.` : `${one.capability}: ${one.why}`)).join(" ");
  }
  if (!prepared.handle) return "Preparing…";
  return null;
}

/** A shape drawn on the canvas as the host takes it, for a location of one of these tools. */
export function shapeOf(mark: Mark, draw: TaskField["draw"]): DrawnShape | null {
  const tools = draw ?? [];
  if (mark.kind === "box" && (tools.includes("box") || tools.includes("polygon"))) {
    if (tools.includes("box")) return { kind: "box", left: mark.x, top: mark.y, right: mark.x + mark.w, bottom: mark.y + mark.h };
    return {
      kind: "polygon",
      points: thinned(
        [
          [mark.x, mark.y],
          [mark.x + mark.w, mark.y],
          [mark.x + mark.w, mark.y + mark.h],
          [mark.x, mark.y + mark.h],
        ],
        POLYGON_POINTS,
      ),
    };
  }
  if (mark.kind === "stroke") {
    const points = thinned(mark.points, PATH_POINTS);
    if (tools.includes("stroke")) return { kind: "stroke", points, radius: Math.max(1, mark.width / 2) };
    if (tools.includes("line")) return { kind: "line", points, width: mark.width };
  }
  return null;
}

/** The most points the host takes in a stroke or a line, and in a polygon. */
export const PATH_POINTS = 512;
export const POLYGON_POINTS = 64;

type P = [number, number];

/** How far `p` is from the segment `a`–`b`. */
function off(p: P, a: P, b: P): number {
  const [dx, dy] = [b[0] - a[0], b[1] - a[1]];
  const length = dx * dx + dy * dy;
  const t = length === 0 ? 0 : Math.max(0, Math.min(1, ((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / length));
  return Math.hypot(p[0] - (a[0] + t * dx), p[1] - (a[1] + t * dy));
}

/** Ramer–Douglas–Peucker: the points that keep the path within `epsilon` of itself. */
function simplified(points: P[], epsilon: number): P[] {
  if (points.length < 3) return points.slice();
  const keep = new Array<boolean>(points.length).fill(false);
  keep[0] = keep[points.length - 1] = true;
  const stack: Array<[number, number]> = [[0, points.length - 1]];
  while (stack.length > 0) {
    const [from, to] = stack.pop()!;
    let far = -1;
    let most = epsilon;
    for (let at = from + 1; at < to; at += 1) {
      const distance = off(points[at]!, points[from]!, points[to]!);
      if (distance > most) {
        most = distance;
        far = at;
      }
    }
    if (far >= 0) {
      keep[far] = true;
      stack.push([from, far], [far, to]);
    }
  }
  return points.filter((_, at) => keep[at]);
}

/**
 * A drawn path with at most `most` points, its first and last kept: points closer than a
 * pixel to the last kept are dropped, the path is simplified to within half a pixel, and
 * what is still too many is taken evenly. A hand drawing a long stroke makes thousands.
 */
export function thinned(points: P[], most: number): P[] {
  if (points.length <= 2) return points.slice();
  const spaced: P[] = [points[0]!];
  for (const point of points.slice(1, -1)) {
    const last = spaced[spaced.length - 1]!;
    if (Math.hypot(point[0] - last[0], point[1] - last[1]) >= 1) spaced.push(point);
  }
  spaced.push(points[points.length - 1]!);
  let kept = simplified(spaced, 0.5);
  if (kept.length > most) {
    const step = (kept.length - 1) / (most - 1);
    kept = Array.from({ length: most }, (_, at) => kept[Math.round(at * step)]!);
  }
  return kept;
}

/** Segment's query from a click or a drag: its kind and its words. */
export function segmentQuery(mark: Mark): { kind: "point" | "box"; query: string } | null {
  if (mark.kind === "point") return { kind: "point", query: `${mark.x} ${mark.y}` };
  if (mark.kind === "box") return { kind: "box", query: `${mark.x} ${mark.y} ${mark.x + mark.w} ${mark.y + mark.h}` };
  return null;
}

/** What a poll heard, kept in order: its lines, and gaps of lines the run did not keep. */
export type Heard = Array<JournalLine | { skipped: number; seq: number }>;

/** Adds one poll's answer to what was heard: a gap first when it skipped any. */
export function hear(heard: Heard, lines: JournalLine[], skipped: number, after: number): Heard {
  const next = [...heard];
  if (skipped > 0) next.push({ skipped, seq: after });
  for (const line of lines) next.push(line);
  return next;
}

/**
 * The step list of a run from its typed events: each step as it started and ended, a
 * divider for each attempt after the first, a gap where events were not kept, and the
 * steps still to come waiting. `waiting` names the task's own steps in order (a recipe's),
 * used while the run is going; a loop's are the steps its first attempt made.
 */
export function stepsOf(heard: Heard, waiting: string[], now: number, going: boolean): StepListEntry[] {
  const entries: StepListEntry[] = [];
  const at = new Map<string, number>();
  const started = new Map<string, number>();
  let attempt = 0;
  const firstAttempt: string[] = [];
  const thisAttempt = new Set<string>();
  let running: number | null = null;
  for (const item of heard) {
    if ("skipped" in item) {
      entries.push({ kind: "skipped", key: `skipped-${item.seq}`, count: item.skipped });
      continue;
    }
    const event = item.event;
    if (!event) continue;
    if (event.kind === "attempt_started") {
      attempt = event.number;
      thisAttempt.clear();
      if (event.number > 1) entries.push({ kind: "attempt", key: `attempt-${event.number}`, number: event.number, change: event.changed });
      continue;
    }
    if (event.kind === "answered") {
      if (running !== null) {
        const step = entries[running];
        if (step && step.kind === "step") step.where = `${event.backend} · ${event.ms} ms`;
      }
      continue;
    }
    const key = `${event.attempt ?? attempt}:${event.step}`;
    if (event.kind === "step_started") {
      if ((event.attempt ?? attempt) <= 1) firstAttempt.push(event.step);
      thisAttempt.add(event.step);
      started.set(key, item.at_ms);
      at.set(key, entries.length);
      running = entries.length;
      entries.push({ kind: "step", key, name: event.step, state: "running", ms: Math.max(0, now - item.at_ms) });
      continue;
    }
    // step_ended
    const ended: StepListEntry = {
      kind: "step",
      key,
      name: event.step,
      state: event.outcome === "completed" ? "done" : "failed",
      ms: started.has(key) ? Math.max(0, item.at_ms - (started.get(key) ?? item.at_ms)) : undefined,
      note: event.outcome === "completed" ? undefined : `${event.outcome} · run ${middle(event.run, 20)}`,
    };
    const place = at.get(key);
    if (place === undefined) {
      thisAttempt.add(event.step);
      entries.push(ended);
    } else {
      const before = entries[place];
      if (before && before.kind === "step" && before.where && ended.kind === "step") ended.where = before.where;
      entries[place] = ended;
    }
    if (running === place) running = null;
  }
  if (going) {
    const still = attempt > 1 ? firstAttempt : waiting;
    // Only the steps after the last one this attempt reached: an attempt that begins
    // later in the task (a retry that keeps what was found) does not wait for the rest.
    const reached = still.reduce((last, name, index) => (thisAttempt.has(name) ? index : last), -1);
    for (const name of still.slice(reached + 1)) {
      if (thisAttempt.has(name)) continue;
      thisAttempt.add(name);
      entries.push({ kind: "step", key: `waiting-${attempt}:${name}`, name, state: "waiting" });
    }
  }
  return entries;
}

/** The attempt a run is on, from its events, and how many it may make, when it says. */
export function attemptOf(heard: Heard): number {
  let attempt = 0;
  for (const item of heard) if (!("skipped" in item) && item.event?.kind === "attempt_started") attempt = item.event.number;
  return attempt;
}

/** The tag a run ends with: its verdict when it has one, else its outcome. */
export function endTag(outcome: string | undefined, verdict: string | null | undefined): "pass" | "fail" | "unsure" | "done" | "cancelled" | "error" {
  if (verdict === "pass" || verdict === "fail" || verdict === "unsure") return verdict;
  if (outcome === "completed") return "done";
  if (outcome === "cancelled") return "cancelled";
  return "error";
}

/** The task a run's action is, among those hello lists. */
export function taskOf(tasks: Task[], action: string | null | undefined): Task | undefined {
  return action ? tasks.find((task) => task.name === action) : undefined;
}

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

/** The backends hello lists for one capability. */
export function backendsFor(capability: string | null | undefined, backends: HelloBackend[]): HelloBackend[] {
  return backends.filter((backend) => backend.capability === capability);
}

/** The choices of a backend field: from its own preparations when there are any, else hello's readiness. */
export function choicesFor(field: TaskField, prepared: Prepared | null, backends: HelloBackend[]): Choice[] {
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
    readiness: readiness(backend.missing, backend.ready),
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
  // A need the call chose no backend for says its model's maker; the plan names the
  // config's backend that answers it, its default.
  const named = (capability: string, backend: string | null | undefined): string => {
    const of = backendsFor(capability, backends);
    if (backend && of.some((one) => one.name === backend)) return backend;
    return (of.find((one) => one.default) ?? of[0])?.name ?? backend ?? "";
  };
  return needs.map((need) => ({
    capability: need.capability,
    backend: named(need.capability, need.backend),
    model: need.model ?? undefined,
    requests: need.requests,
    each: need.each,
    total: need.bound,
    local: need.local,
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
    return missing.map((one) => `${one.capability}: ${one.why}`).join(" ");
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
      points: [
        [mark.x, mark.y],
        [mark.x + mark.w, mark.y],
        [mark.x + mark.w, mark.y + mark.h],
        [mark.x, mark.y + mark.h],
      ],
    };
  }
  if (mark.kind === "stroke") {
    if (tools.includes("stroke")) return { kind: "stroke", points: mark.points, radius: Math.max(1, mark.width / 2) };
    if (tools.includes("line")) return { kind: "line", points: mark.points, width: mark.width };
  }
  return null;
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

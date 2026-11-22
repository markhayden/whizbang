// What the Models screen reads and asks of the host (plan m5, task M1; ADR 0041, "Routes"):
// the catalogue's models, an install's progress, Get, Cancel and Remove; each backend the
// config offers and whether its key is set (never the key); and the allowance, with the
// owner's reset. Each answer is read defensively: a field of another type is as if absent.
import type { Client } from "../../api";
import type { ModelState } from "../../components/ModelRow";
import { dollars } from "../../format";

/** One model of the catalogue, as `GET /api/models` lists it. */
export interface ModelInfo {
  name: string;
  title?: string;
  licence?: string;
  bytes?: number;
  files?: number;
  /** `installed`, `not_installed`, `damaged`, `out_of_date`, `being_removed`, … */
  state?: string;
  why?: string;
  /** While an install runs: the files it has staged. */
  files_done?: number | null;
  /** `none`, or why the download's sign-in is missing: `not_named`, `not_set`. */
  sign_in?: string;
  /** This host's install of it: `installing` or `stopping`. */
  install?: string;
  /** How this host's last install of it ended. */
  last?: { ended: string; why?: string | null };
}

export interface ModelsAnswer {
  models: ModelInfo[];
  installing?: string | null;
}

/** One backend the config offers, as `GET /api/backends` lists it. */
export interface BackendInfo {
  capability: string;
  name: string;
  kind?: string;
  model?: string | null;
  default?: boolean;
  local?: boolean;
  /** `key_set`, `no_key`, `not_set_up`, `no_key_needed` or `local`. */
  access?: string;
  price?: { kind: "free" | "at_most" | "about"; usd?: number | null } | null;
}

interface Summed {
  usd: number;
  stopped?: boolean;
}

/** The allowance, as `GET /api/spend` answers it. */
export interface Spend {
  epoch: number;
  allowance: number | null;
  since_ms: number | null;
  counted: { exactly: boolean; sum: Summed };
  charged: Summed;
  held: Summed;
  left: number;
  needs_owner?: boolean;
  call_cap: number | null;
  config: string | null;
}

const isObject = (value: unknown): value is Record<string, unknown> => typeof value === "object" && value !== null && !Array.isArray(value);

/** The models of an answer: only rows that name themselves. */
export function modelsOf(answer: unknown): ModelInfo[] {
  if (!isObject(answer) || !Array.isArray(answer.models)) return [];
  return answer.models.filter((row): row is ModelInfo => isObject(row) && typeof row.name === "string");
}

export function backendsOf(answer: unknown): BackendInfo[] {
  if (!isObject(answer) || !Array.isArray(answer.backends)) return [];
  return answer.backends.filter((row): row is BackendInfo => isObject(row) && typeof row.name === "string" && typeof row.capability === "string");
}

/** Whether an install is going for this model, here. */
export function installing(model: ModelInfo): boolean {
  return model.install === "installing" || model.install === "stopping" || typeof model.files_done === "number";
}

/** The row's state for the model row component. */
export function stateOf(model: ModelInfo): ModelState {
  if (installing(model)) return { kind: "installing", filesDone: model.files_done ?? 0, files: model.files ?? 0 };
  switch (model.state) {
    case "installed":
      return { kind: "ready" };
    case "damaged":
      return { kind: "damaged" };
    case "not_installed":
    case "out_of_date":
      return model.sign_in && model.sign_in !== "none" ? { kind: "not-set-up" } : { kind: "get" };
    default:
      return { kind: "not-set-up" };
  }
}

/** What a model's row says under its name: its title and licence, then why it is as it is. */
export function lineOf(model: ModelInfo, usedBy: string[]): string {
  const parts: string[] = [];
  if (model.title) parts.push(model.title);
  if (usedBy.length > 0) parts.push(`used by ${usedBy.join(", ")}`);
  if (installing(model)) {
    parts.push(model.install === "stopping" ? "cancelling" : "installing: counted in files, not bytes");
  } else if (model.sign_in === "not_named") {
    parts.push("its download needs a sign-in the config does not name");
  } else if (model.sign_in === "not_set") {
    parts.push("its download needs a token the env file does not set");
  } else if (model.state && model.state !== "installed" && model.state !== "not_installed" && model.why) {
    parts.push(model.why);
  }
  if (!installing(model) && model.last && model.last.ended !== "installed" && model.last.why) parts.push(model.last.why);
  return parts.join(" · ");
}

/** A backend's state for the model row component. */
export function backendState(backend: BackendInfo): ModelState {
  switch (backend.access) {
    case "key_set":
      return { kind: "key-set" };
    case "no_key":
      return { kind: "no-key" };
    case "no_key_needed":
    case "local":
      return { kind: "ready" };
    default:
      return { kind: "not-set-up" };
  }
}

/** What a backend's row says: its capability and its price, as the backend states it. */
export function backendLine(backend: BackendInfo): string {
  const parts = [backend.default ? `${backend.capability} · the default` : backend.capability];
  if (backend.model) parts.push(backend.model);
  const price = backend.price;
  if (price?.kind === "free") parts.push("free");
  else if (price && typeof price.usd === "number") parts.push(`${price.kind === "about" ? "about" : "at most"} ${dollars(price.usd)} a request`);
  return parts.join(" · ");
}

/** A date as the allowance says it: `2026-11-02`. */
export function dayOf(ms: number): string {
  return new Date(ms).toISOString().slice(0, 10);
}

/** Whether text is an allowance the host takes: dollars, with at most six digits after a point. */
export function isAllowance(text: string): boolean {
  return /^\d+(\.\d{1,6})?$/.test(text);
}

export const listBackends = (client: Client) => client.request<{ backends: BackendInfo[] }>("GET", "/api/backends");
export const readSpend = (client: Client) => client.request<Spend>("GET", "/api/spend");
export const getModel = (client: Client, name: string) => client.request<{ attached: boolean }>("POST", `/api/models/${encodeURIComponent(name)}/get`);
export const cancelInstall = (client: Client, name: string) => client.request("POST", `/api/models/${encodeURIComponent(name)}/cancel`);
export const removeModel = (client: Client, name: string) => client.request("DELETE", `/api/models/${encodeURIComponent(name)}`);
/** The owner's reset: the amount as typed, and the word as typed, which the host checks again. */
export const resetAllowance = (client: Client, allowance: string, confirm: string) =>
  client.request<{ allowance: number; epoch: number }>("POST", "/api/spend/reset", { allowance, confirm });

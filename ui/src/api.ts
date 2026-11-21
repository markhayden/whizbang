// The page's client of the playground API (plan m5, "The playground API"; ADR 0041). Every
// call carries the launch token in `Authorization: Bearer`, never in a cookie. A refusal
// becomes an `ApiError` whose sentence the page shows. Uploads are held to two at once and
// the rest queued, so of Chrome's six connections to the host at least four are left for
// polls, Stop and Quit. A run open on screen is polled every 250 ms with the events after
// the last one the page has.

/** What `GET /api/hello` answers (plan m5, task W1, item 2). */
export interface Hello {
  version: string;
  store?: string;
  kits?: HelloKit[];
  /** The tools (primitives), each with the kit its tasks use it in, or none. */
  tools?: Task[];
  /** Always an array from the host; read through `backendsOf`, which holds to that. */
  backends?: HelloBackend[];
  /** Why the backends' readiness could not be read, when it could not. */
  backends_error?: string;
  allowance?: { allowance: number; left: number; since?: string };
}

export interface HelloKit {
  id: string;
  name?: string;
  line?: string;
  /** How many tools the kit's tasks use. */
  tools?: number;
  tasks?: Task[];
}

/** A task (a recipe or a loop) or a tool (a primitive), with the form that runs it. */
export interface Task {
  name: string;
  kit?: string | null;
  title?: string;
  kind?: "recipe" | "loop" | "primitive";
  purpose?: string;
  use?: string | null;
  /** The other kit a loop uses, by its id. */
  uses?: string | null;
  shape?: string;
  /** The actions its steps call, in order. */
  tools?: string[];
  fields?: TaskField[];
  outputs?: Array<{ name: string; type: string }>;
  plans?: boolean;
}

export type FieldKind = "text" | "number" | "integer" | "boolean" | "choice" | "backend" | "picture" | "pictures" | "artifact";

/** One input of a task's form, from its descriptor (or, for squint's use cases, the host's form). */
export interface TaskField {
  name: string;
  label?: string;
  purpose?: string;
  kind: FieldKind;
  required?: boolean;
  default?: unknown;
  min?: number | null;
  max?: number | null;
  choices?: string[] | null;
  /** For a backend field: the capability it chooses for. */
  capability?: string | null;
  /** For a picture, pictures or artifact field: the artifact type it takes. */
  artifact?: string | null;
  count?: { min: number; max: number | null } | null;
  /** For a location: what may be drawn on the canvas. */
  draw?: Array<"box" | "polygon" | "stroke" | "line"> | null;
  /** The picture field it is drawn on. */
  on?: string | null;
  /** `point_or_box`: segment's query, filled from a click or a drag on the canvas. */
  canvas?: "point_or_box" | null;
}

export interface HelloBackend {
  name: string;
  ready: boolean;
  capability?: string;
  kind?: string;
  model?: string | null;
  default?: boolean;
  missing?: string | null;
  why?: string;
  size?: number | null;
}

/** The backends hello lists: an empty list for anything that is not an array. */
export function backendsOf(hello: Hello | null | undefined): HelloBackend[] {
  return Array.isArray(hello?.backends) ? hello.backends : [];
}

/** Every task and tool hello lists, with the kit each is under (a tool's may be none). */
export function tasksOf(hello: Hello | null | undefined): Task[] {
  const kits = Array.isArray(hello?.kits) ? hello.kits : [];
  const tasks = kits.flatMap((kit) => (Array.isArray(kit.tasks) ? kit.tasks.map((task) => ({ ...task, kit: task.kit ?? kit.id })) : []));
  const tools = Array.isArray(hello?.tools) ? hello.tools : [];
  return [...tasks, ...tools];
}

/** One need of a preparation, as the plan block shows it. */
export interface PreparedNeed {
  capability: string;
  backend: string;
  model?: string | null;
  requests: number;
  each: number;
  bound: number;
  local?: boolean;
}

/** One choice of a backend field, prepared alone. */
export interface PreparedChoice {
  value: string;
  ready: boolean;
  kind: string;
  why?: string | null;
  size?: number | null;
}

/** What `POST /api/prepare` answers. */
export interface Prepared {
  ready: boolean;
  handle?: string | null;
  refused?: boolean;
  kind?: string | null;
  why?: string | null;
  missing?: Array<{ capability: string; kind: string; why: string }>;
  needs?: PreparedNeed[];
  bound?: number;
  attempts?: { asked: number; allowed: number } | null;
  choices?: Record<string, PreparedChoice[]>;
}

/** A typed event beside a journal line's message (K1). */
export type RunEvent =
  | { kind: "step_started"; step: string; action: string; attempt?: number }
  | { kind: "step_ended"; step: string; outcome: string; run: string; attempt?: number }
  | { kind: "attempt_started"; number: number; changed?: string }
  | { kind: "answered"; capability: string; backend: string; model: string; ms: number; ended: string };

export interface JournalLine {
  seq: number;
  at_ms: number;
  message: string;
  event?: RunEvent | null;
}

export interface RunOutput {
  port: string;
  type: string;
  id: string;
}

/** What `GET /api/runs/{id}?after=` answers. */
export interface Polled {
  run: string;
  state: "running" | "stopping" | "ended" | "not_here" | "abandoned" | "removed";
  outcome?: string;
  events: JournalLine[];
  next: number;
  skipped: number;
  action?: string | null;
  arguments?: Record<string, unknown> | null;
  spent?: number;
  reserved?: number;
  outputs?: RunOutput[];
  verdict?: "pass" | "fail" | "unsure" | null;
}

/** One run still going, as `GET /api/runs?open=1` lists it. */
export interface OpenRun {
  run: string;
  action: string;
  started_ms: number;
  state: string;
}

/** A picture the store holds, as the playground imported it. */
export interface Picture {
  id: string;
  name: string;
  width?: number;
  height?: number;
  at_ms?: number;
}

/** A shape drawn on the canvas, in the picture's pixels, as `POST /api/locations` takes it. */
export type DrawnShape =
  | { kind: "box"; left: number; top: number; right: number; bottom: number }
  | { kind: "polygon"; points: Array<[number, number]> }
  | { kind: "stroke"; points: Array<[number, number]>; radius: number }
  | { kind: "line"; points: Array<[number, number]>; width: number };

/** A refusal or a failure, with the sentence the page shows. */
export class ApiError extends Error {
  readonly status: number;
  /** The kind of refusal the host named (`stale`, `conflict`, …), when it named one. */
  readonly kind: string | undefined;
  constructor(status: number, sentence: string, kind?: string) {
    super(sentence);
    this.status = status;
    this.kind = kind;
    this.name = "ApiError";
  }
}

/** The sentence for a status when the host gave none. */
export function sentenceFor(status: number): string {
  switch (status) {
    case 0:
      return "The playground's host is not answering. Run whizbang play again.";
    case 401:
      return "This page's key is not the host's: it is from an earlier launch, or missing. Run whizbang play again, which opens the page with a new one.";
    case 403:
      return "The host refused a request from this page.";
    case 409:
      return "What this would run has changed since it was prepared, or another run holds its id. Prepare it again and run it.";
    case 413:
      return "That is too large for the host.";
    case 415:
      return "The host does not take that kind of file.";
    case 429:
      return "The host is busy; try again in a moment.";
    case 503:
      return "The host is stopping.";
    default:
      return `The host answered ${status}.`;
  }
}

/** A queue that runs at most `limit` tasks at once, in the order they were added. */
export class Limiter {
  private running = 0;
  private readonly waiting: Array<() => void> = [];
  constructor(private readonly limit: number) {}
  get active(): number {
    return this.running;
  }
  get queued(): number {
    return this.waiting.length;
  }
  run<T>(task: () => Promise<T>): Promise<T> {
    return new Promise<T>((resolve, reject) => {
      const start = () => {
        this.running += 1;
        task()
          .then(resolve, reject)
          .finally(() => {
            this.running -= 1;
            this.waiting.shift()?.();
          });
      };
      if (this.running < this.limit) start();
      else this.waiting.push(start);
    });
  }
}

/** What `POST /api/runs` is given: the call, the handle of its preparation, its request id. */
export interface RunStart {
  action: string;
  arguments: Record<string, unknown>;
  /** The handle `POST /api/prepare` answered for exactly these arguments. */
  handle: string;
}

/** What `POST /api/runs` answers: the run it started, or the one its request id started. */
export interface Started {
  run: string;
}

/**
 * A new request id: the page makes one for each Run pressed and sends it again on every
 * retry of that start, so the host answers the run it started rather than a second one
 * (ADR 0007; plan m5, task W1, item 1).
 */
export function newRequestId(random: () => string = () => globalThis.crypto.randomUUID()): string {
  return `play-${random()}`;
}

/** How often a start whose answer was lost is sent again, with the same request id. */
export const START_TRIES = 3;

/** The most uploads the page sends at once (plan m5, "Who may call"). */
export const UPLOADS_AT_ONCE = 2;
/** How often a run open on screen is asked for (plan m5, "Live progress is polled"). */
export const POLL_MS = 250;
/** The longest a poll waits between two asks while the host answers 503. */
export const POLL_MAX_MS = 4000;

/** The wait before a start is sent again: 250 to 500 ms. */
export function retryPause(random: () => number = Math.random): number {
  return 250 + Math.floor(random() * 250);
}

export interface ClientOptions {
  token: string | null;
  fetch?: typeof fetch;
  /** Called once a call is refused 401, so the page can say to run `whizbang play` again. */
  onUnauthorized?: (error: ApiError) => void;
  /** Waits that many milliseconds; a test passes one that does not wait. */
  pause?: (ms: number) => Promise<void>;
}

export interface Client {
  /** Use another token from now on: one a later `#k=` in the address handed the page. */
  setToken(token: string | null): void;
  request<T>(method: string, path: string, body?: unknown): Promise<T>;
  hello(): Promise<Hello>;
  upload<T>(path: string, file: Blob): Promise<T>;
  /**
   * Start a run under one request id, made here unless given, and sent again with the same
   * id, 250 to 500 ms later, when the answer was lost on the way: no status, or a 2xx whose
   * body could not be read. A refusal is not retried: 409 says
   * the preparation is stale (prepare again, then start with a new id) or that the id
   * names another call; 429 says four runs are going.
   */
  start(call: RunStart, requestId?: string): Promise<Started & { requestId: string }>;
  /**
   * Ask every 250 ms with the events after the last the page has, until `next` says the run
   * is done or the poll is stopped. A refusal (4xx) other than 429 stops it; a 503 (the host
   * stopping) or a 429 doubles the wait, up to 4 s; any other failure keeps the pace.
   */
  poll<T>(path: (after: number) => string, next: (answer: T) => { after: number; done: boolean }, onError?: (error: ApiError) => void): () => void;
  /** Prepare a call: readiness per choice, each need's bound, a handle when it is ready. */
  prepare(action: string, args: Record<string, unknown>): Promise<Prepared>;
  /** Follow a run: every 250 ms, the events after the last the page has, until it ends. */
  pollRun(run: string, onAnswer: (answer: Polled) => void, onError?: (error: ApiError) => void): () => void;
  /** Stop a run: the host answers "stopping" at once. */
  cancelRun(run: string): Promise<{ run: string; state: string }>;
  /** The runs this host started that are still going. */
  openRuns(): Promise<OpenRun[]>;
  /** Send a picture's bytes with its file name; at most two at once, the rest queued. */
  uploadPicture(file: File): Promise<Picture>;
  /** The pictures the playground imported, newest first. */
  listPictures(limit?: number): Promise<Picture[]>;
  /** A shape drawn on a picture, committed as a location: its identity. */
  commitLocation(picture: string, shape: DrawnShape): Promise<{ id: string }>;
  /** An address the page can show bytes of the API from (`<img>` sends no token). */
  blobUrl(path: string): Promise<string>;
}

async function refusal(response: Response): Promise<ApiError> {
  let sentence = sentenceFor(response.status);
  let kind: string | undefined;
  try {
    const body = (await response.json()) as { error?: unknown; kind?: unknown };
    if (typeof body.error === "string" && body.error.length > 0 && response.status !== 401) sentence = body.error;
    if (typeof body.kind === "string") kind = body.kind;
  } catch {
    // No JSON body: the sentence for the status stands.
  }
  return new ApiError(response.status, sentence, kind);
}

export function createClient(options: ClientOptions): Client {
  const call = options.fetch ?? ((input, init) => globalThis.fetch(input, init));
  const pause = options.pause ?? ((ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms)));
  const uploads = new Limiter(UPLOADS_AT_ONCE);
  let token = options.token;
  const headers = (extra: Record<string, string>): Record<string, string> => {
    const all: Record<string, string> = { ...extra };
    if (token) all.Authorization = `Bearer ${token}`;
    return all;
  };
  const send = async <T>(method: string, path: string, body: BodyInit | undefined, type: string | undefined): Promise<T> => {
    let response: Response;
    try {
      response = await call(path, { method, headers: headers(type ? { "Content-Type": type } : {}), body, cache: "no-store", credentials: "omit", redirect: "error" });
    } catch {
      throw new ApiError(0, sentenceFor(0));
    }
    if (!response.ok) {
      const error = await refusal(response);
      if (error.status === 401) options.onUnauthorized?.(error);
      throw error;
    }
    if (response.status === 204) return undefined as T;
    try {
      return (await response.json()) as T;
    } catch {
      // The host answered, and the answer was lost on the way: as if no answer came.
      throw new ApiError(0, sentenceFor(0));
    }
  };
  const blobs = new Map<string, Promise<string>>();
  const client: Client = {
    setToken: (next) => {
      token = next;
    },
    request: <T>(method: string, path: string, body?: unknown) =>
      send<T>(method, path, body === undefined ? undefined : JSON.stringify(body), body === undefined ? undefined : "application/json"),
    hello: () => client.request<Hello>("GET", "/api/hello"),
    start: async (call: RunStart, requestId: string = newRequestId()) => {
      for (let attempt = 1; ; attempt += 1) {
        try {
          const started = await client.request<Started>("POST", "/api/runs", { ...call, request_id: requestId });
          return { ...started, requestId };
        } catch (error) {
          if (!(error instanceof ApiError) || error.status !== 0 || attempt >= START_TRIES) throw error;
        }
        await pause(retryPause());
      }
    },
    upload: <T>(path: string, file: Blob) => uploads.run(() => send<T>("POST", path, file, file.type || "application/octet-stream")),
    prepare: (action, args) => client.request<Prepared>("POST", "/api/prepare", { action, arguments: args }),
    pollRun: (run, onAnswer, onError) =>
      client.poll<Polled>(
        (after) => `/api/runs/${encodeURIComponent(run)}?after=${after}`,
        (answer) => {
          onAnswer(answer);
          return { after: answer.next, done: answer.state !== "running" && answer.state !== "stopping" };
        },
        onError,
      ),
    cancelRun: (run) => client.request("POST", `/api/runs/${encodeURIComponent(run)}/cancel`),
    openRuns: async () => {
      const answer = await client.request<{ runs?: OpenRun[] } | undefined>("GET", "/api/runs?open=1");
      return Array.isArray(answer?.runs) ? answer.runs : [];
    },
    uploadPicture: (file) => client.upload<Picture>(`/api/pictures?name=${encodeURIComponent(file.name.slice(0, 200))}`, file),
    listPictures: async (limit = 48) => {
      const answer = await client.request<{ pictures?: Picture[] } | undefined>("GET", `/api/pictures?limit=${limit}`);
      return Array.isArray(answer?.pictures) ? answer.pictures : [];
    },
    commitLocation: (picture, shape) => client.request<{ id: string }>("POST", "/api/locations", { picture, shape }),
    blobUrl: (path) => {
      const known = blobs.get(path);
      if (known) return known;
      const made = (async () => {
        let response: Response;
        try {
          response = await call(path, { method: "GET", headers: headers({}), cache: "no-store", credentials: "omit", redirect: "error" });
        } catch {
          throw new ApiError(0, sentenceFor(0));
        }
        if (!response.ok) throw await refusal(response);
        return URL.createObjectURL(await response.blob());
      })();
      made.catch(() => blobs.delete(path));
      blobs.set(path, made);
      return made;
    },
    poll: <T>(path: (after: number) => string, next: (answer: T) => { after: number; done: boolean }, onError?: (error: ApiError) => void) => {
      let after = 0;
      let stopped = false;
      let wait = POLL_MS;
      let timer: ReturnType<typeof setTimeout> | undefined;
      const tick = async () => {
        if (stopped) return;
        try {
          const answer = await client.request<T>("GET", path(after));
          if (stopped) return;
          const said = next(answer);
          after = said.after;
          wait = POLL_MS;
          if (said.done) return;
        } catch (error) {
          if (stopped) return;
          const refused = error instanceof ApiError ? error : new ApiError(0, sentenceFor(0));
          onError?.(refused);
          if (refused.status >= 400 && refused.status < 500 && refused.status !== 429) return;
          wait = refused.status === 503 || refused.status === 429 ? Math.min(POLL_MAX_MS, wait * 2) : POLL_MS;
        }
        timer = setTimeout(() => void tick(), wait);
      };
      void tick();
      return () => {
        stopped = true;
        if (timer !== undefined) clearTimeout(timer);
      };
    },
  };
  return client;
}

/** An artifact's identity as the host writes it. */
export const IDENTITY = /^sha256:[0-9a-f]{64}$/;

/** Where the page reads an artifact's bytes, or its thumbnail at most `edge` pixels. */
export function artifactPath(id: string, edge?: number): string {
  if (!IDENTITY.test(id)) throw new Error("not an artifact identity");
  return edge === undefined ? `/api/artifacts/${id}` : `/api/artifacts/${id}/thumb?edge=${Math.round(edge)}`;
}

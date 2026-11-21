// The page's client of the playground API (plan m5, "The playground API"; ADR 0041). Every
// call carries the launch token in `Authorization: Bearer`, never in a cookie. A refusal
// becomes an `ApiError` whose sentence the page shows. Uploads are held to two at once and
// the rest queued, so of Chrome's six connections to the host at least four are left for
// polls, Stop and Quit. A run open on screen is polled every 250 ms with the events after
// the last one the page has.

/** What `GET /api/hello` answers. Fields that later tasks add are optional here. */
export interface Hello {
  version: string;
  store?: string;
  kits?: HelloKit[];
  backends?: HelloBackend[];
  allowance?: { allowance: number; left: number; since?: string };
}

export interface HelloKit {
  id: string;
  name?: string;
  line?: string;
  tasks?: Array<{ name: string; title?: string; shape?: string; uses?: string }>;
}

export interface HelloBackend {
  name: string;
  ready: boolean;
  why?: string;
}

/** A refusal or a failure, with the sentence the page shows. */
export class ApiError extends Error {
  readonly status: number;
  constructor(status: number, sentence: string) {
    super(sentence);
    this.status = status;
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

/** The most uploads the page sends at once (plan m5, "Who may call"). */
export const UPLOADS_AT_ONCE = 2;
/** How often a run open on screen is asked for (plan m5, "Live progress is polled"). */
export const POLL_MS = 250;

export interface ClientOptions {
  token: string | null;
  fetch?: typeof fetch;
  /** Called once a call is refused 401, so the page can say to run `whizbang play` again. */
  onUnauthorized?: (error: ApiError) => void;
}

export interface Client {
  request<T>(method: string, path: string, body?: unknown): Promise<T>;
  hello(): Promise<Hello>;
  upload<T>(path: string, file: Blob): Promise<T>;
  poll<T>(path: (after: number) => string, next: (answer: T) => { after: number; done: boolean }, onError?: (error: ApiError) => void): () => void;
}

async function refusal(response: Response): Promise<ApiError> {
  let sentence = sentenceFor(response.status);
  try {
    const body = (await response.json()) as { error?: unknown };
    if (typeof body.error === "string" && body.error.length > 0 && response.status !== 401) sentence = body.error;
  } catch {
    // No JSON body: the sentence for the status stands.
  }
  return new ApiError(response.status, sentence);
}

export function createClient(options: ClientOptions): Client {
  const call = options.fetch ?? ((input, init) => globalThis.fetch(input, init));
  const uploads = new Limiter(UPLOADS_AT_ONCE);
  const headers = (extra: Record<string, string>): Record<string, string> => {
    const all: Record<string, string> = { ...extra };
    if (options.token) all.Authorization = `Bearer ${options.token}`;
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
    return (await response.json()) as T;
  };
  const client: Client = {
    request: <T>(method: string, path: string, body?: unknown) =>
      send<T>(method, path, body === undefined ? undefined : JSON.stringify(body), body === undefined ? undefined : "application/json"),
    hello: () => client.request<Hello>("GET", "/api/hello"),
    upload: <T>(path: string, file: Blob) => uploads.run(() => send<T>("POST", path, file, file.type || "application/octet-stream")),
    poll: <T>(path: (after: number) => string, next: (answer: T) => { after: number; done: boolean }, onError?: (error: ApiError) => void) => {
      let after = 0;
      let stopped = false;
      let timer: ReturnType<typeof setTimeout> | undefined;
      const tick = async () => {
        if (stopped) return;
        try {
          const answer = await client.request<T>("GET", path(after));
          if (stopped) return;
          const said = next(answer);
          after = said.after;
          if (said.done) return;
        } catch (error) {
          if (stopped) return;
          onError?.(error instanceof ApiError ? error : new ApiError(0, sentenceFor(0)));
          if (error instanceof ApiError && error.status === 401) return;
        }
        timer = setTimeout(() => void tick(), POLL_MS);
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

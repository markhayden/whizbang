// The page's routes, by the address's hash (plan m5, task U1, item 4): `#/` (with
// `?picture=<identity>`, a picture of the library picked, to start a task with),
// `#/k/<kit>/<task>`, `#/run/<id>`, `#/history`, `#/models`; and task W3's
// `#/run/<id>/report` and `#/record/<name>`; plan m7, task U1's `#/compare/<baseline>`
// and `#/compare/<baseline>/<attempt>`; plan m7, task U2's `#/bundles`, `#/bundles/<digest>`
// and `#/chain/<digest>`. A hash keeps every route on
// the one page the host serves, so a reload or a copied address finds the same screen.

export type Route =
  | { name: "home"; picture?: string }
  | { name: "kit"; kit: string; task?: string }
  | { name: "run"; id: string }
  | { name: "history"; query: URLSearchParams }
  | { name: "models" }
  | { name: "report"; id: string }
  | { name: "record"; record: string }
  | { name: "compare"; baseline: string; attempt?: string }
  | { name: "bundles"; digest?: string }
  | { name: "chain"; digest: string }
  | { name: "unknown"; hash: string };

/** A name in an address: lower-case letters, digits, `_` and `-` (the rule for names). */
const NAME = /^[a-z][a-z0-9_-]{0,63}$/;
/** A run's id: letters, digits, `-` and `_`. */
const RUN = /^[A-Za-z0-9_-]{1,128}$/;
/** A picture's identity in the store, and a bundle's digest. */
const PICTURE = /^sha256:[0-9a-f]{64}$/;

export function parseRoute(hash: string): Route {
  const raw = hash.replace(/^#/, "");
  const [path = "", search = ""] = raw.split("?", 2);
  const parts = path.split("/").filter(Boolean);
  if (parts.length === 0) {
    const picture = new URLSearchParams(search).get("picture");
    return picture && PICTURE.test(picture) ? { name: "home", picture } : { name: "home" };
  }
  let decoded: string[];
  try {
    decoded = parts.map((part) => decodeURIComponent(part));
  } catch {
    // A bad escape (`%E0%A4%A`) names no route.
    return { name: "unknown", hash };
  }
  const [head, a, b] = decoded;
  if (head === "k" && a && NAME.test(a) && parts.length <= 3) {
    if (b === undefined) return { name: "kit", kit: a };
    if (NAME.test(b)) return { name: "kit", kit: a, task: b };
  }
  if (head === "run" && a && RUN.test(a) && parts.length === 2) return { name: "run", id: a };
  if (head === "run" && a && RUN.test(a) && b === "report" && parts.length === 3) return { name: "report", id: a };
  if (head === "record" && a && NAME.test(a) && parts.length === 2) return { name: "record", record: a };
  if (head === "history" && parts.length === 1) return { name: "history", query: new URLSearchParams(search) };
  if (head === "models" && parts.length === 1) return { name: "models" };
  if (head === "compare" && a && RUN.test(a) && parts.length === 2) return { name: "compare", baseline: a };
  if (head === "compare" && a && RUN.test(a) && b && RUN.test(b) && parts.length === 3) return { name: "compare", baseline: a, attempt: b };
  if (head === "bundles" && parts.length === 1) return { name: "bundles" };
  if (head === "bundles" && a && PICTURE.test(a) && parts.length === 2) return { name: "bundles", digest: a };
  if (head === "chain" && a && PICTURE.test(a) && parts.length === 2) return { name: "chain", digest: a };
  return { name: "unknown", hash };
}

export function hrefOf(route: Route): string {
  switch (route.name) {
    case "home":
      return route.picture ? `#/?picture=${route.picture}` : "#/";
    case "kit":
      return route.task ? `#/k/${encodeURIComponent(route.kit)}/${encodeURIComponent(route.task)}` : `#/k/${encodeURIComponent(route.kit)}`;
    case "run":
      return `#/run/${encodeURIComponent(route.id)}`;
    case "history": {
      const search = route.query.toString();
      return search ? `#/history?${search}` : "#/history";
    }
    case "models":
      return "#/models";
    case "report":
      return `#/run/${encodeURIComponent(route.id)}/report`;
    case "record":
      return `#/record/${encodeURIComponent(route.record)}`;
    case "compare":
      return route.attempt ? `#/compare/${encodeURIComponent(route.baseline)}/${encodeURIComponent(route.attempt)}` : `#/compare/${encodeURIComponent(route.baseline)}`;
    case "bundles":
      return route.digest ? `#/bundles/${route.digest}` : "#/bundles";
    case "chain":
      return `#/chain/${route.digest}`;
    case "unknown":
      return route.hash || "#/";
  }
}

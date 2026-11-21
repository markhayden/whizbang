// The page's routes, by the address's hash (plan m5, task U1, item 4): `#/`,
// `#/k/<kit>/<task>`, `#/run/<id>`, `#/history`, `#/models`. A hash keeps every route on
// the one page the host serves, so a reload or a copied address finds the same screen.

export type Route =
  | { name: "home" }
  | { name: "kit"; kit: string; task?: string }
  | { name: "run"; id: string }
  | { name: "history"; query: URLSearchParams }
  | { name: "models" }
  | { name: "unknown"; hash: string };

/** A name in an address: lower-case letters, digits, `_` and `-` (the rule for names). */
const NAME = /^[a-z][a-z0-9_-]{0,63}$/;
/** A run's id: letters, digits, `-` and `_`. */
const RUN = /^[A-Za-z0-9_-]{1,128}$/;

export function parseRoute(hash: string): Route {
  const raw = hash.replace(/^#/, "");
  const [path = "", search = ""] = raw.split("?", 2);
  const parts = path.split("/").filter(Boolean);
  if (parts.length === 0) return { name: "home" };
  const [head, a, b] = parts.map((part) => decodeURIComponent(part));
  if (head === "k" && a && NAME.test(a) && parts.length <= 3) {
    if (b === undefined) return { name: "kit", kit: a };
    if (NAME.test(b)) return { name: "kit", kit: a, task: b };
  }
  if (head === "run" && a && RUN.test(a) && parts.length === 2) return { name: "run", id: a };
  if (head === "history" && parts.length === 1) return { name: "history", query: new URLSearchParams(search) };
  if (head === "models" && parts.length === 1) return { name: "models" };
  return { name: "unknown", hash };
}

export function hrefOf(route: Route): string {
  switch (route.name) {
    case "home":
      return "#/";
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
    case "unknown":
      return route.hash || "#/";
  }
}

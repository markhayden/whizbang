// The launch token (plan m5, "The playground API", "The token"). `whizbang play` opens
// `http://127.0.0.1:<port>/#k=<token>`; the page reads the token from the address, keeps
// it in its own storage for that origin, and removes it from the address bar at once.

/** Where the page keeps the token. */
export const TOKEN_KEY = "whizbang.token";

/** A token as the host makes it: 32 bytes as 64 hexadecimal characters. */
const TOKEN = /^#k=([0-9a-f]{64})$/;

export interface Place {
  hash: string;
  pathname: string;
  search: string;
}

export interface Keeper {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
}

/**
 * The token in the address, kept and taken out of the address; else the one kept before;
 * else null. `replace` is `history.replaceState`'s address part.
 */
export function takeToken(place: Place, keeper: Keeper, replace: (url: string) => void): string | null {
  const found = TOKEN.exec(place.hash);
  if (found && found[1]) {
    try {
      keeper.setItem(TOKEN_KEY, found[1]);
    } catch {
      // Storage refused (a private window): the token lives for this page only.
    }
    replace(`${place.pathname}${place.search}#/`);
    return found[1];
  }
  try {
    return keeper.getItem(TOKEN_KEY);
  } catch {
    return null;
  }
}

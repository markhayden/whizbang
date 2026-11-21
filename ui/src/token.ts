// The launch token (plan m5, "The playground API", "The token"). `whizbang play` opens
// `http://127.0.0.1:<port>/#k=<token>`; the page reads the token from the address, keeps
// it in the tab's session storage, and removes it from the address bar at once. Session
// storage lasts for a reload of the same tab and ends with it: each tab `whizbang play`
// opens carries its own `#k=`, and no other tab, page or later launch reads it.
//
// An address whose hash begins with `#k=` is never left in the bar, whether or not what
// follows is a token, and the same holds for one that arrives after the page loaded (a
// hash change): the page acts on nothing else in its address (H1's host lets a top-level
// navigation reach it).

/** Where the page keeps the token. */
export const TOKEN_KEY = "whizbang.token";

/** A token as the host makes it: 32 bytes as 64 hexadecimal characters. */
const TOKEN = /^#k=([0-9a-fA-F]{64})$/;
/** Any hash that tries to hand the page a token. */
const OFFERED = /^#k=/i;

export interface Place {
  hash: string;
  pathname: string;
  search: string;
}

export interface Keeper {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
}

export interface Taken {
  /** The token to use: the one the address carried, else the one kept before, else null. */
  token: string | null;
  /** The address carried a token (valid or not), and it was taken out of the bar. */
  offered: boolean;
  /** The address carried a valid token, now kept. */
  fresh: boolean;
}

/**
 * The token in the address, kept and taken out of the address; else the one kept before;
 * else null. `replace` is `history.replaceState`'s address part.
 */
export function takeToken(place: Place, keeper: Keeper, replace: (url: string) => void): Taken {
  const kept = (): string | null => {
    try {
      return keeper.getItem(TOKEN_KEY);
    } catch {
      return null;
    }
  };
  // Read before the address is replaced: `place` may be the live `location`.
  const hash = place.hash;
  if (!OFFERED.test(hash)) return { token: kept(), offered: false, fresh: false };
  const found = TOKEN.exec(hash);
  replace(`${place.pathname}${place.search}#/`);
  if (!found || !found[1]) return { token: kept(), offered: true, fresh: false };
  const token = found[1].toLowerCase();
  try {
    keeper.setItem(TOKEN_KEY, token);
  } catch {
    // Storage refused: the token lives for this page only.
  }
  return { token, offered: true, fresh: true };
}

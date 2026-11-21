// The keyboard (plan m5, task U1, item 4): ⌘K search, ⌘↵ Run, ⌘S Save, ⌘1 to ⌘9 the kits
// in the switcher's order, ⌘0 Home. Control does what ⌘ does, for Linux.

export type KeyAction = { kind: "search" } | { kind: "run" } | { kind: "save" } | { kind: "kit"; index: number } | { kind: "home" };

export interface Keys {
  key: string;
  metaKey: boolean;
  ctrlKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
}

export function actionOf(event: Keys): KeyAction | null {
  if (!(event.metaKey || event.ctrlKey) || event.altKey) return null;
  const key = event.key.length === 1 ? event.key.toLowerCase() : event.key;
  if (event.shiftKey) return null;
  if (key === "k") return { kind: "search" };
  if (key === "Enter") return { kind: "run" };
  if (key === "s") return { kind: "save" };
  if (key === "0") return { kind: "home" };
  if (/^[1-9]$/.test(key)) return { kind: "kit", index: Number(key) - 1 };
  return null;
}

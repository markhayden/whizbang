// How numbers read on screen: money, sizes, times and counts. Pure, so each is tested
// alone (format.test.ts).

/** US dollars, with three decimals under one dollar and two above: `$0.040`, `$3.18`. */
export function dollars(value: number): string {
  if (!Number.isFinite(value)) return "$—";
  const abs = Math.abs(value);
  const text = abs < 1 ? abs.toFixed(3) : abs.toFixed(2);
  return `${value < 0 ? "−" : ""}$${text}`;
}

/** A size in bytes as a person reads it, in decimal units: `3.4 GB`, `820 MB`. */
export function bytes(value: number): string {
  if (!Number.isFinite(value) || value < 0) return "—";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let size = value;
  let unit = 0;
  while (size >= 1000 && unit < units.length - 1) {
    size /= 1000;
    unit += 1;
  }
  const text = unit === 0 || size >= 100 ? size.toFixed(0) : size.toFixed(1);
  return `${text} ${units[unit]}`;
}

/** Elapsed milliseconds as seconds with one decimal under a minute: `4.1 s`, `2 m 05 s`. */
export function elapsed(ms: number): string {
  if (!Number.isFinite(ms) || ms < 0) return "—";
  const seconds = ms / 1000;
  if (seconds < 60) return `${seconds.toFixed(1)} s`;
  const whole = Math.floor(seconds);
  const minutes = Math.floor(whole / 60);
  return `${minutes} m ${String(whole % 60).padStart(2, "0")} s`;
}

/** How long ago, from two times in milliseconds: `now`, `8m ago`, `3h ago`, `2d ago`. */
export function ago(then: number, now: number): string {
  const seconds = Math.max(0, Math.round((now - then) / 1000));
  if (seconds < 45) return "now";
  const minutes = Math.round(seconds / 60);
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.round(minutes / 60);
  if (hours < 48) return `${hours}h ago`;
  return `${Math.round(hours / 24)}d ago`;
}

/** A fraction from 0 to 1 as a whole percent, held to that range. */
export function percent(fraction: number): number {
  if (!Number.isFinite(fraction)) return 0;
  return Math.round(Math.min(1, Math.max(0, fraction)) * 100);
}

/**
 * A long id cut in the middle where space is short, for display only: what is copied is
 * always the whole id. Ids of `max` characters or fewer are shown whole.
 */
export function middle(id: string, max = 14): string {
  if (id.length <= max) return id;
  const keep = Math.max(2, max - 1);
  const head = Math.ceil(keep / 2);
  const tail = Math.floor(keep / 2);
  return `${id.slice(0, head)}…${id.slice(id.length - tail)}`;
}

import { useEffect, useRef, useState } from "preact/hooks";
import { cx } from "./cx";

export interface SearchEntry {
  /** Unique among the entries. */
  key: string;
  kind: "task" | "tool" | "picture";
  label: string;
  /** A second line: the kit, the shape, the file's size. */
  detail?: string;
}

export interface SearchPaletteProps {
  entries: SearchEntry[];
  /** At most this many matches are listed. */
  limit?: number;
  onPick?: (key: string) => void;
  onClose?: () => void;
}

/** The entries whose label or detail holds every word of the query, without case. */
export function matching(entries: SearchEntry[], query: string, limit = 12): SearchEntry[] {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean);
  return entries.filter((entry) => words.every((word) => `${entry.label} ${entry.detail ?? ""}`.toLowerCase().includes(word))).slice(0, limit);
}

const KIND_WORD = { task: "task", tool: "tool", picture: "picture" };

/** ⌘K: search tasks, tools and pictures; ↑ ↓ to move, ↵ to open, Esc to close. */
export function SearchPalette(props: SearchPaletteProps) {
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);
  const input = useRef<HTMLInputElement>(null);
  useEffect(() => input.current?.focus(), []);
  const found = matching(props.entries, query, props.limit);
  return (
    <div class="palette-scrim" onClick={props.onClose}>
      <div class="menu palette" role="dialog" aria-modal="true" aria-label="Search" onClick={(event) => event.stopPropagation()}>
        <input
          ref={input}
          class="palette__input"
          type="search"
          placeholder="Search tasks, tools and pictures"
          aria-label="Search tasks, tools and pictures"
          value={query}
          onInput={(event) => {
            setQuery((event.currentTarget as HTMLInputElement).value);
            setActive(0);
          }}
          onKeyDown={(event) => {
            if (event.key === "Escape") props.onClose?.();
            if (event.key === "ArrowDown") setActive((at) => Math.min(found.length - 1, at + 1));
            if (event.key === "ArrowUp") setActive((at) => Math.max(0, at - 1));
            if (event.key === "Enter" && found[active]) props.onPick?.(found[active].key);
          }}
        />
        <ul class="palette__list" role="listbox">
          {found.length === 0 ? <li class="menu__empty">Nothing matches.</li> : null}
          {found.map((entry, index) => (
            <li key={entry.key} role="option" aria-selected={index === active ? "true" : "false"}>
              <button type="button" class={cx("menu__item", index === active && "menu__item--active")} onClick={() => props.onPick?.(entry.key)}>
                <span class="menu__text">
                  <span class="menu__title">{entry.label}</span>
                  {entry.detail ? <span class="menu__line">{entry.detail}</span> : null}
                </span>
                <span class="menu__key mono">{KIND_WORD[entry.kind]}</span>
              </button>
            </li>
          ))}
        </ul>
      </div>
    </div>
  );
}

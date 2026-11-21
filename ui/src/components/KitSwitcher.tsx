import { cx } from "./cx";
import type { KitTone } from "./kitTone";

export interface KitEntry {
  id: string;
  name: string;
  line: string;
  tone: KitTone;
}

export interface KitSwitcherProps {
  kits: KitEntry[];
  current?: string;
  /** A line after the current kit's, and after the others': "same picture stays open". */
  note?: string;
  onPick?: (id: string) => void;
  onHome?: () => void;
}

/** The key that opens the kit at a place in the switcher: ⌘1 to ⌘9, then none. */
export function kitKey(index: number): string | undefined {
  return index < 9 ? `⌘${index + 1}` : undefined;
}

/** The kits, in the description's order, with ⌘1 to ⌘9, and Home with ⌘0. */
export function KitSwitcher(props: KitSwitcherProps) {
  return (
    <div class="menu kit-switcher" role="menu" aria-label="Kits">
      <div class="menu__heading">Kits</div>
      {props.kits.map((kit, index) => {
        const current = kit.id === props.current;
        const key = kitKey(index);
        return (
          <button key={kit.id} type="button" role="menuitem" class={cx("menu__item", current && "menu__item--current")} onClick={() => props.onPick?.(kit.id)}>
            <span class={`menu__swatch tone-fill--${kit.tone}`} aria-hidden="true" />
            <span class="menu__text">
              <span class={cx("menu__title", current && "menu__title--strong")}>{kit.name}</span>
              <span class="menu__line">
                {kit.line}
                {current ? " · current" : props.note ? ` · ${props.note}` : ""}
              </span>
            </span>
            {key ? <span class="menu__key mono">{key}</span> : null}
          </button>
        );
      })}
      <button type="button" role="menuitem" class="menu__foot" onClick={props.onHome}>
        <span>Home</span>
        <span class="menu__key mono">⌘0</span>
      </button>
    </div>
  );
}

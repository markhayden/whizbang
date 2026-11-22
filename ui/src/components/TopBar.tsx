import type { ComponentChildren } from "preact";
import { dollars } from "../format";
import { cx } from "./cx";
import { Wordmark } from "./Wordmark";

export interface TopBarProps {
  /** The middle of the bar's left side: "Home", or the breadcrumb of a kit. */
  where: ComponentChildren;
  /**
   * What is spent and held since the last reset, and the allowance, in US dollars; `null`
   * when the host said the store has no allowance yet (no reset has set one), `undefined`
   * while it has not said.
   */
  spent?: number;
  /** What runs still going hold beyond what they spent. */
  held?: number;
  /** False when the count is only a lower bound: then it says "at least". */
  exactly?: boolean;
  allowance?: number | null;
  current?: "history" | "models";
  homeHref: string;
  historyHref: string;
  modelsHref: string;
  wordmark?: string;
  onSearch?: () => void;
  /** Quit: asks first, then stops the host (`POST /api/quit`). */
  onQuit?: () => void;
}

/** The top bar: the wordmark, where you are, ⌘K search, History, Models, the allowance, Quit. */
export function TopBar(props: TopBarProps) {
  return (
    <header class="top-bar">
      <div class="top-bar__start">
        <a class="top-bar__home" href={props.homeHref} aria-label="whizbang, home">
          <Wordmark src={props.wordmark} />
        </a>
        {props.where}
      </div>
      <button type="button" class="top-bar__search" onClick={props.onSearch}>
        <span>Search tasks, tools and pictures</span>
        <span class="mono">⌘K</span>
      </button>
      <nav class="top-bar__end" aria-label="Pages">
        <a class={cx("top-bar__link", props.current === "history" && "top-bar__link--current")} href={props.historyHref}>
          History
        </a>
        <a class={cx("top-bar__link", props.current === "models" && "top-bar__link--current")} href={props.modelsHref}>
          Models
        </a>
        {typeof props.allowance === "number" && typeof props.spent === "number" ? (
          <span class="top-bar__allowance mono" title="Spent since the last reset, and held by runs still going, of the allowance it set">
            {props.exactly === false ? "at least " : ""}
            {dollars(props.spent)} <span class="top-bar__of">spent</span>
            {props.held ? (
              <>
                {" · "}
                {dollars(props.held)} <span class="top-bar__of">held</span>
              </>
            ) : null}{" "}
            <span class="top-bar__of">of {dollars(props.allowance)}</span>
          </span>
        ) : props.allowance === null ? (
          <span class="top-bar__allowance mono" title="No reset has set an allowance, so every paid run is refused">
            <span class="top-bar__of">no allowance</span>
          </span>
        ) : null}
        {props.onQuit ? (
          <button type="button" class="top-bar__link top-bar__quit" onClick={props.onQuit}>
            Quit
          </button>
        ) : null}
      </nav>
    </header>
  );
}

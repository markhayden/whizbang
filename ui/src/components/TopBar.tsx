import type { ComponentChildren } from "preact";
import { dollars } from "../format";
import { cx } from "./cx";
import { Wordmark } from "./Wordmark";

export interface TopBarProps {
  /** The middle of the bar's left side: "Home", or the breadcrumb of a kit. */
  where: ComponentChildren;
  /** What is left of the allowance, and the allowance, in US dollars. */
  left?: number;
  allowance?: number;
  current?: "history" | "models";
  homeHref: string;
  historyHref: string;
  modelsHref: string;
  wordmark?: string;
  onSearch?: () => void;
}

/** The top bar: the wordmark, where you are, ⌘K search, History, Models, the allowance. */
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
        {props.left !== undefined && props.allowance !== undefined ? (
          <span class="top-bar__allowance mono" title="What is left of the allowance since the last reset">
            {dollars(props.left)} <span class="top-bar__of">/ {props.allowance.toFixed(2)}</span>
          </span>
        ) : null}
      </nav>
    </header>
  );
}

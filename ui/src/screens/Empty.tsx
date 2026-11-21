import type { ComponentChildren } from "preact";

export interface EmptyScreenProps {
  title: string;
  children: ComponentChildren;
  /** A way on: an address and its words. */
  link?: { href: string; words: string };
}

/** A screen with nothing to show yet: its title, why, and a way on. */
export function EmptyScreen(props: EmptyScreenProps) {
  return (
    <main class="screen screen--empty" aria-labelledby="screen-title">
      <h1 id="screen-title" class="screen__title">
        {props.title}
      </h1>
      <p class="empty">{props.children}</p>
      {props.link ? (
        <a class="screen__link" href={props.link.href}>
          {props.link.words}
        </a>
      ) : null}
    </main>
  );
}

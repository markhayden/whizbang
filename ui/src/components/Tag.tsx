import { percent } from "../format";

export type TagKind = "draft" | "running" | "pass" | "fail" | "unsure" | "no-key" | "interrupted" | "free-local" | "cancelled" | "done" | "error";

const WORDS: Record<TagKind, string> = {
  draft: "DRAFT",
  running: "RUNNING",
  pass: "PASS",
  fail: "FAIL",
  unsure: "UNSURE",
  "no-key": "NO KEY",
  interrupted: "INTERRUPTED",
  "free-local": "FREE · LOCAL",
  cancelled: "CANCELLED",
  done: "DONE",
  error: "ERROR",
};

export interface TagProps {
  kind: TagKind;
  /** For a running tag: how far, from 0 to 1. Without it the tag says RUNNING alone. */
  progress?: number;
  /** A soft tag (a tint behind coloured words) instead of a filled one, as in rows. */
  soft?: boolean;
  /** Words after the kind, such as "2 TRIES". */
  detail?: string;
}

/** The words a tag says. */
export function tagWords(props: TagProps): string {
  let words = WORDS[props.kind];
  if (props.kind === "running" && props.progress !== undefined) words += ` · ${percent(props.progress)}%`;
  if (props.detail) words += ` · ${props.detail.toUpperCase()}`;
  return words;
}

/** A status: DRAFT, RUNNING (with its progress), PASS, FAIL, UNSURE, NO KEY, … */
export function Tag(props: TagProps) {
  return <span class={`tag tag--${props.kind}${props.soft ? " tag--soft" : ""} mono`}>{tagWords(props)}</span>;
}

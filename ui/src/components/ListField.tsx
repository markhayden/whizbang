import type { ComponentChildren } from "preact";
import { Field } from "./Field";

export interface ListItem {
  /** Unique within the list. */
  key: string;
  /** What the row says, as text. */
  label: string;
  /** A small picture beside it, when the item is a picture. */
  thumb?: string;
}

export interface ListFieldProps {
  id: string;
  label: string;
  items: ListItem[];
  /** The fewest and the most items the port takes (its count). */
  min: number;
  max: number | null;
  help?: string;
  error?: string;
  disabled?: boolean;
  /** The control that adds an item: a drop target or a picker. Hidden at the most. */
  adder?: ComponentChildren;
  onMove?: (from: number, to: number) => void;
  onRemove?: (index: number) => void;
}

/** What the list says about its count: "2 of at most 4", "at least 1". */
export function countLine(length: number, min: number, max: number | null): string {
  if (max !== null && min === max) return `${length} of ${max}`;
  if (max !== null) return `${length} · ${min} to ${max}`;
  return `${length} · at least ${min}`;
}

/** An ordered list held to its count: items can be moved and removed; an adder adds. */
export function ListField(props: ListFieldProps) {
  const full = props.max !== null && props.items.length >= props.max;
  const short = props.items.length < props.min;
  return (
    <Field id={props.id} label={props.label} help={props.help ?? countLine(props.items.length, props.min, props.max)} error={props.error} disabled={props.disabled}>
      <ol class="list-field" id={props.id} data-short={short ? "true" : undefined}>
        {props.items.map((item, index) => (
          <li class="list-field__item" key={item.key}>
            <span class="list-field__index mono">{index + 1}</span>
            {item.thumb ? <img class="list-field__thumb" src={item.thumb} alt="" /> : null}
            <span class="list-field__label">{item.label}</span>
            {props.disabled ? null : (
              <span class="list-field__actions">
                <button type="button" class="icon-button" aria-label={`Move ${item.label} up`} disabled={index === 0} onClick={() => props.onMove?.(index, index - 1)}>
                  ↑
                </button>
                <button
                  type="button"
                  class="icon-button"
                  aria-label={`Move ${item.label} down`}
                  disabled={index === props.items.length - 1}
                  onClick={() => props.onMove?.(index, index + 1)}
                >
                  ↓
                </button>
                <button type="button" class="icon-button" aria-label={`Remove ${item.label}`} onClick={() => props.onRemove?.(index)}>
                  ✕
                </button>
              </span>
            )}
          </li>
        ))}
      </ol>
      {full || props.disabled ? null : props.adder}
    </Field>
  );
}

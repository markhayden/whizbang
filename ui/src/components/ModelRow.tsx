import { bytes } from "../format";
import { ProgressBar } from "./ProgressBar";

/**
 * A model or a backend on the Models screen. On this Mac: installed (READY), not installed
 * (Get), installing (files done of files) or damaged. In the cloud: KEY SET, NO KEY or NOT
 * SET UP, never the key itself.
 */
export type ModelState =
  | { kind: "ready" }
  | { kind: "get" }
  | { kind: "installing"; filesDone: number; files: number }
  | { kind: "damaged" }
  | { kind: "key-set" }
  | { kind: "no-key" }
  | { kind: "not-set-up" };

export interface ModelRowProps {
  name: string;
  /** What it does, or its price: "finds things you name", "judges · ~$0.002 a check". */
  line: string;
  /** Its size on disk, for a local model. */
  size?: number;
  state: ModelState;
  onGet?: () => void;
  onCancel?: () => void;
  onRemove?: () => void;
  onHowToAdd?: () => void;
}

const SOFT: Partial<Record<ModelState["kind"], { words: string; tone: string }>> = {
  ready: { words: "READY", tone: "pass" },
  "key-set": { words: "KEY SET", tone: "pass" },
  "no-key": { words: "NO KEY", tone: "lacquer" },
  damaged: { words: "DAMAGED", tone: "lacquer" },
  "not-set-up": { words: "NOT SET UP", tone: "muted" },
};

/** A row of the Models screen, with the one action its state offers. */
export function ModelRow(props: ModelRowProps) {
  const soft = SOFT[props.state.kind];
  const state = props.state;
  return (
    <div class="model-row">
      <span class="model-row__text">
        <span class="model-row__name">{props.name}</span>
        <span class="model-row__line">{props.line}</span>
        {state.kind === "installing" ? (
          <ProgressBar label={`${props.name} installing`} value={state.files > 0 ? state.filesDone / state.files : 0} figures={`${state.filesDone} of ${state.files} files`} title="" />
        ) : null}
      </span>
      {props.size !== undefined ? <span class="model-row__size mono">{bytes(props.size)}</span> : null}
      {soft ? <span class={`model-row__state mono soft--${soft.tone}`}>{soft.words}</span> : null}
      {state.kind === "get" && props.onGet ? (
        <button type="button" class="small-button" onClick={props.onGet}>
          Get
        </button>
      ) : null}
      {state.kind === "installing" && props.onCancel ? (
        <button type="button" class="small-button" onClick={props.onCancel}>
          Cancel
        </button>
      ) : null}
      {(state.kind === "ready" || state.kind === "damaged") && props.onRemove ? (
        <button type="button" class="small-button" onClick={props.onRemove}>
          Remove
        </button>
      ) : null}
      {(state.kind === "no-key" || state.kind === "not-set-up") && props.onHowToAdd ? (
        <button type="button" class="small-button" onClick={props.onHowToAdd}>
          How to add
        </button>
      ) : null}
    </div>
  );
}

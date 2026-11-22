// The outputs of runs this page saw end, offered to a port of their type by the binding
// picker (plan m5, task W1, item 4): what the page can bind before history (W3) lists every
// run's outputs.
import type { RunOutput } from "./api";
import type { Binding } from "./components/BindingPicker";

const seen: Binding[] = [];
/** The most outputs remembered: the oldest go first. */
export const SEEN_LIMIT = 64;

/** Remembers the outputs of a run that ended, newest first, each once. */
export function rememberOutputs(run: string, task: string, outputs: RunOutput[], when: string): void {
  for (const output of outputs) {
    if (seen.some((binding) => binding.id === output.id && binding.run === run)) continue;
    seen.unshift({ id: output.id, type: output.type, run, task, when });
  }
  seen.splice(SEEN_LIMIT);
}

/** The outputs remembered, newest first. */
export function seenOutputs(): Binding[] {
  return [...seen];
}

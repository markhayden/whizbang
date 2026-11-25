import { useEffect, useState } from "preact/hooks";
import { ApiError, type Client } from "../api";
import { bundleAbout, bundleRun, sizeWords, type BundleAbout } from "../bundles";
import { save } from "../screens/history/calls";
import { Button } from "./Button";
import { Dialog } from "./Dialog";
import "./bundles.css";

export interface BundleDialogProps {
  client: Client;
  run: string;
  onCancel: () => void;
}

/** "N things" in words. */
function counted(count: number, one: string, many = `${one}s`): string {
  return `${count} ${count === 1 ? one : many}`;
}

/**
 * "Bundle this run" (plan m7, task U2, item 1; the design's surface 3, A): what the bundle
 * holds and never holds, its size before it is written as about the sum of what the store
 * holds of the run (the plan's G1 table), and the exact size once it is.
 */
export function BundleDialog(props: BundleDialogProps) {
  const [about, setAbout] = useState<BundleAbout | null>(null);
  const [exact, setExact] = useState<number | null>(null);
  const [busy, setBusy] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  useEffect(() => {
    let live = true;
    bundleAbout(props.client, props.run).then(
      (answer) => live && setAbout(answer),
      (error: unknown) => live && setProblem(error instanceof ApiError ? error.message : String(error)),
    );
    return () => {
      live = false;
    };
  }, [props.run]);
  const name = `${props.run}.whizbang-bundle`;
  const download = () => {
    setBusy(true);
    setProblem(null);
    bundleRun(props.client, props.run).then(
      (blob) => {
        setBusy(false);
        setExact(blob.size);
        save(blob, name);
      },
      (error: unknown) => {
        setBusy(false);
        setProblem(error instanceof ApiError ? error.message : String(error));
      },
    );
  };
  const size = exact !== null ? sizeWords(exact) : about ? sizeWords(about.about_bytes, true) : "measuring…";
  return (
    <Dialog
      id="bundle-title"
      title={`Bundle run ${props.run}`}
      line={`${name} · ${size}`}
      class="bundle-dialog"
      onCancel={props.onCancel}
      foot={
        <>
          <Button variant="quiet" onClick={props.onCancel}>
            {exact !== null ? "Close" : "Cancel"}
          </Button>
          <Button variant="primary" busy={busy} disabled={!about} why={about ? undefined : "Reading what the run holds."} onClick={download}>
            {exact !== null ? "Download again" : "Download bundle"}
          </Button>
        </>
      }
    >
      <section class="holds" aria-label="Holds">
        <p class="holds__title">Holds</p>
        <p class="holds__item mono">
          <span class="holds__yes" aria-hidden="true">✓</span> {about ? `the run's record${about.runs > 1 ? ` and ${counted(about.runs - 1, "run")} under it` : ""}` : "the run's record and its steps"}
        </p>
        <p class="holds__item mono">
          <span class="holds__yes" aria-hidden="true">✓</span> {about ? `${counted(about.inputs, "input")}, ${counted(about.outputs, "output")}${about.masks ? `, ${counted(about.masks, "mask")}` : ""}` : "its inputs and outputs"}
        </p>
        <p class="holds__item mono">
          <span class="holds__yes" aria-hidden="true">✓</span> each model's answer, for offline replay
        </p>
        <p class="holds__title">Never holds</p>
        <p class="holds__item holds__item--never mono">— keys, tokens or endpoints</p>
        <p class="holds__item holds__item--never mono">— paths on this machine</p>
        <p class="holds__item holds__item--never mono">— model weights</p>
        <p class="holds__note">It is not encrypted: it holds the run's pictures and words. Send it as you would send the pictures.</p>
      </section>
      {exact !== null ? (
        <p class="holds__note" role="status">
          {`Written: ${sizeWords(exact)}. Take it into another playground on its Bundles page.`}
        </p>
      ) : null}
      {problem ? (
        <p class="field__error" role="alert">
          {problem}
        </p>
      ) : null}
    </Dialog>
  );
}

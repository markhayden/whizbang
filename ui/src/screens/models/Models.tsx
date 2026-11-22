import { useEffect, useState } from "preact/hooks";
import { ApiError, type Client } from "../../api";
import { AllowanceBar } from "../../components/AllowanceBar";
import { Button } from "../../components/Button";
import { ModelRow } from "../../components/ModelRow";
import { TextField } from "../../components/TextField";
import { bytes, dollars } from "../../format";
import {
  backendLine,
  backendState,
  backendsOf,
  cancelInstall,
  dayOf,
  getModel,
  cancellable,
  installing,
  isAllowance,
  lineOf,
  listBackends,
  modelsOf,
  readSpend,
  removeModel,
  resetAllowance,
  stateOf,
  type BackendInfo,
  type ModelInfo,
  type Spend,
} from "./catalogue";
import "./models.css";

export interface ModelsProps {
  client: Client;
}

const sentence = (error: unknown) => (error instanceof ApiError ? error.message : String(error));

/**
 * Models (plan m5, task M1): what runs on this Mac and in the cloud, and the allowance. Keys
 * live in the config's env file and are never shown, sent to the page or written by it; the
 * page installs, cancels and removes models, and resets the allowance, and nothing else.
 */
export function Models(props: ModelsProps) {
  const { client } = props;
  const [models, setModels] = useState<ModelInfo[] | null>(null);
  const [backends, setBackends] = useState<BackendInfo[]>([]);
  const [spend, setSpend] = useState<Spend | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [asked, setAsked] = useState(0);
  const [helping, setHelping] = useState(false);
  /** The model whose Remove was pressed, waiting for its confirmation. */
  const [removing, setRemoving] = useState<string | null>(null);

  // The models, asked for again (every 250 ms) while an install runs.
  useEffect(
    () =>
      client.poll<unknown>(
        () => "/api/models",
        (answer) => {
          const listed = modelsOf(answer);
          setModels(listed);
          return { after: 0, done: !listed.some(installing) };
        },
        (error) => setProblem(error.message),
      ),
    [client, asked],
  );
  useEffect(() => {
    let live = true;
    // Asked through a resolved promise, so that a client that answers nothing yet (a
    // test's) is an empty screen and not a fault.
    Promise.resolve()
      .then(() => listBackends(client))
      .then(
      (answer) => live && setBackends(backendsOf(answer)),
      (error: unknown) => live && setProblem(sentence(error)),
    );
    Promise.resolve()
      .then(() => readSpend(client))
      .then(
      (answer) => live && setSpend(answer ?? null),
      (error: unknown) => live && setProblem(sentence(error)),
    );
    return () => {
      live = false;
    };
  }, [client, asked]);

  const again = () => setAsked((n) => n + 1);
  const act = (doing: () => Promise<unknown>) =>
    void Promise.resolve()
      .then(doing)
      .then(
      () => {
        setProblem(null);
        again();
      },
      (error: unknown) => {
        setProblem(sentence(error));
        again();
      },
    );

  const cloud = backends.filter((backend) => !backend.local);
  const usedBy = (model: string) => backends.filter((backend) => backend.local && backend.model === model).map((backend) => `${backend.capability} · ${backend.name}`);
  const onDisk = (models ?? []).filter((model) => model.state === "installed").reduce((sum, model) => sum + (model.bytes ?? 0), 0);

  return (
    <main class="screen models" aria-labelledby="screen-title">
      <div class="models__head">
        <h1 id="screen-title" class="screen__title">
          Models
        </h1>
        <p class="models__lead">What can run, where, and what it costs. Keys live in your env file and are never shown here.</p>
      </div>
      {problem ? (
        <p class="models__problem" role="alert">
          {problem}
        </p>
      ) : null}
      <div class="models__panels">
        <section class="models__panel" aria-labelledby="models-mac">
          <header class="models__panel-head">
            <h2 id="models-mac" class="models__panel-title">
              On your Mac
            </h2>
            <span class="models__panel-note mono">free · private · {bytes(onDisk)} used</span>
          </header>
          {models === null ? <p class="models__empty">Looking at the models directory…</p> : null}
          {models !== null && models.length === 0 ? <p class="models__empty">This build's catalogue holds no model.</p> : null}
          {(models ?? []).map((model) => (
            <ModelRow
              key={model.name}
              name={model.name}
              line={lineOf(model, usedBy(model.name))}
              size={model.bytes}
              state={stateOf(model)}
              onGet={() => act(() => getModel(client, model.name))}
              onCancel={cancellable(model) ? () => act(() => cancelInstall(client, model.name)) : undefined}
              onRemove={() => setRemoving(model.name)}
              onHowToAdd={model.sign_in && model.sign_in !== "none" ? () => setHelping(true) : undefined}
            />
          ))}
          {removing ? (
            <div class="models__confirm" role="group" aria-label={`Remove ${removing}`}>
              <p class="models__confirm-words">
                Remove <span class="mono">{removing}</span>? Its files are deleted. Runs that use it are waited for five seconds; if they still hold it, nothing is removed.
              </p>
              <div class="models__confirm-actions">
                <Button
                  variant="danger"
                  onClick={() => {
                    const name = removing;
                    setRemoving(null);
                    act(() => removeModel(client, name));
                  }}
                >
                  Remove {removing}
                </Button>
                <Button variant="quiet" onClick={() => setRemoving(null)}>
                  Keep it
                </Button>
              </div>
            </div>
          ) : null}
        </section>
        <section class="models__panel" aria-labelledby="models-cloud">
          <header class="models__panel-head">
            <h2 id="models-cloud" class="models__panel-title">
              In the cloud
            </h2>
            <span class="models__panel-note mono">your keys · paid per use</span>
          </header>
          {cloud.length === 0 ? <p class="models__empty">The config offers no backend in the cloud.</p> : null}
          {cloud.map((backend) => (
            <ModelRow key={`${backend.capability}/${backend.name}`} name={backend.name} line={backendLine(backend)} state={backendState(backend)} onHowToAdd={() => setHelping(true)} />
          ))}
        </section>
      </div>
      {helping ? <HowToAdd config={spend?.config ?? null} onClose={() => setHelping(false)} /> : null}
      <Allowance spend={spend} client={client} onReset={again} />
    </main>
  );
}

/** How a key or a sign-in is added: in the config and the env file, by hand, never here. */
function HowToAdd(props: { config: string | null; onClose: () => void }) {
  return (
    <section class="models__help" aria-labelledby="models-help">
      <h2 id="models-help" class="models__panel-title">
        How to add a key or a sign-in
      </h2>
      <p>
        The playground never asks for a key and never writes one. A backend in the cloud names the variable that holds its key in the config{props.config ? ` (${props.config})` : ""}, as{" "}
        <span class="mono">key_env = "NAME"</span>; the key itself goes in the env file, as <span class="mono">NAME=…</span>, and nowhere else.
      </p>
      <p>
        A model whose files need a sign-in names where its token is under <span class="mono">[[models.tokens]]</span> in the config, as a variable of the env file (the playground does not read the Hugging Face sign-in;
        <span class="mono"> whizbang models get</span> does).
        Then start the playground again with the same <span class="mono">--config</span> and <span class="mono">--env-file</span>.
      </p>
      <p class="models__help-guide">The guide: docs/guides/playground.md, "Models and keys".</p>
      <Button variant="quiet" onClick={props.onClose}>
        Close
      </Button>
    </section>
  );
}

/** The allowance since the last reset, the cap of one call (read-only), and the owner's reset. */
function Allowance(props: { spend: Spend | null; client: Client; onReset: () => void }) {
  const { spend } = props;
  const [step, setStep] = useState<"idle" | "asking" | "sending">("idle");
  const [amount, setAmount] = useState("");
  const [word, setWord] = useState("");
  const [refused, setRefused] = useState<string | null>(null);
  const [done, setDone] = useState<string | null>(null);

  const since = spend?.since_ms ? `since ${dayOf(spend.since_ms)}` : "since the last reset";
  const spent = spend?.counted?.sum?.usd ?? 0;
  const ready = isAllowance(amount) && word === "reset";
  const why = !isAllowance(amount) ? "Type the new allowance in dollars, as 20 or 12.50." : word !== "reset" ? "Type the word reset to confirm." : undefined;

  const send = () => {
    setStep("sending");
    resetAllowance(props.client, amount, word).then(
      (opened) => {
        setStep("idle");
        setAmount("");
        setWord("");
        setRefused(null);
        setDone(`The books are closed: the allowance is ${dollars(opened.allowance)} from now on.`);
        props.onReset();
      },
      (error: unknown) => {
        setStep("asking");
        setRefused(sentence(error));
      },
    );
  };

  return (
    <section class="models__allowance" aria-labelledby="models-allowance">
      <div class="models__allowance-row">
        <div class="models__allowance-words">
          <h2 id="models-allowance" class="models__panel-title">
            Allowance
          </h2>
          <span class="models__panel-sub">Runs stop before going over it.</span>
        </div>
        <div class="models__allowance-bar">
          {spend && typeof spend.allowance === "number" ? (
            <AllowanceBar allowance={spend.allowance} spent={spent} since={`spent ${since}${spend.counted?.exactly === false ? ", at least" : ""}`} />
          ) : (
            <span class="models__panel-sub">{spend ? "No allowance yet: every paid run is refused until a reset sets one." : "Reading the store's books…"}</span>
          )}
        </div>
        <div class="models__allowance-figures">
          <span class="models__chip" title={spend?.config ? `From ${spend.config}; the playground does not change it.` : "Set in the config; the playground does not change it."}>
            <span class="models__chip-label">Per run</span>
            <span class="mono">{typeof spend?.call_cap === "number" ? dollars(spend.call_cap) : "no cap"}</span>
          </span>
          {step === "idle" ? (
            <Button
              variant="secondary"
              onClick={() => {
                setDone(null);
                setStep("asking");
              }}
            >
              Reset…
            </Button>
          ) : null}
        </div>
      </div>
      {done ? (
        <p class="models__panel-sub" role="status">
          {done}
        </p>
      ) : null}
      {step !== "idle" ? (
        <div class="models__reset" role="group" aria-label="Reset the allowance">
          <p class="models__reset-words">
            A reset closes the books: what was spent {since} stops counting, and the new allowance starts with nothing spent. Runs already admitted keep what they reserved, under
            the books they were admitted under. It is the same as{" "}
            <span class="mono">whizbang spend reset</span>.
          </p>
          <div class="models__reset-fields">
            <TextField id="reset-amount" label="New allowance, in US dollars" value={amount} placeholder="20" maxLength={24} onInput={setAmount} disabled={step === "sending"} />
            <TextField id="reset-word" label="Type reset to confirm" value={word} placeholder="reset" maxLength={16} onInput={setWord} disabled={step === "sending"} error={refused ?? undefined} />
          </div>
          <div class="models__reset-actions">
            <Button variant="danger" disabled={!ready} why={why} busy={step === "sending"} onClick={send}>
              {step === "sending" ? "Resetting…" : `Reset to ${isAllowance(amount) ? dollars(Number(amount)) : "…"}`}
            </Button>
            <Button
              variant="quiet"
              disabled={step === "sending"}
              onClick={() => {
                setStep("idle");
                setRefused(null);
              }}
            >
              Keep it
            </Button>
          </div>
        </div>
      ) : null}
    </section>
  );
}

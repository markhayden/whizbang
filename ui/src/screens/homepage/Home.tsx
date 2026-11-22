import { useEffect, useState } from "preact/hooks";
import { ApiError, tasksOf, type Client, type Hello, type OpenRun } from "../../api";
import { BrandArt } from "../../components/BrandArt";
import { ContinueRow } from "../../components/ContinueRow";
import { DropTarget } from "../../components/DropTarget";
import { KitPanel } from "../../components/KitPanel";
import { kitTone } from "../../components/kitTone";
import { Wordmark } from "../../components/Wordmark";
import { readDrafts, localStore, type Draft } from "../../drafts";
import { taskOf } from "../../form";
import { ago } from "../../format";
import { usePictures } from "../../hooks";
import { hrefOf } from "../../routes";
import { StoredPicture } from "../shared";

export interface HomeProps {
  hello: Hello | null;
  /** The client of the API; without one (a test of the shell alone) Home asks nothing. */
  client?: Client;
  go?: (hash: string) => void;
}

const STEPS = [
  { n: "01", title: "Pick a kit", line: "each kit is a set of tasks" },
  { n: "02", title: "Give it a picture", line: "say or draw what you want" },
  { n: "03", title: "Send it on", line: "check · fix · copy command" },
];

/** How often Home asks again for the runs still going. */
const OPEN_EVERY_MS = 2000;

/** Home: the intro, Continue, the kits and their tasks, the pictures. */
export function Home(props: HomeProps) {
  const kits = Array.isArray(props.hello?.kits) ? props.hello.kits : [];
  const tasks = tasksOf(props.hello);
  const [open, setOpen] = useState<OpenRun[]>([]);
  const [drafts] = useState<Draft[]>(() => readDrafts(localStore()));
  const [problem, setProblem] = useState<string | null>(null);
  const [asked, setAsked] = useState(0);
  const client = props.client;

  useEffect(() => {
    if (!client) return;
    let live = true;
    const look = () =>
      client.openRuns().then(
        (runs) => live && setOpen(runs),
        () => undefined,
      );
    void look();
    const timer = setInterval(() => void look(), OPEN_EVERY_MS);
    return () => {
      live = false;
      clearInterval(timer);
    };
  }, [client]);

  const library = usePictures(client, asked);
  const pictures = library.pictures;

  const upload = (files: File[]) => {
    if (!client) return;
    setProblem(null);
    void Promise.all(
      files.map((file) =>
        client.uploadPicture(file).catch((error: unknown) => {
          setProblem(error instanceof ApiError ? error.message : String(error));
        }),
      ),
    ).then(() => setAsked((n) => n + 1));
  };

  const now = Date.now();
  const titled = (action: string) => taskOf(tasks, action);
  return (
    <main class="screen home" aria-labelledby="home-title">
      <section class="home__intro">
        <div class="home__lead">
          <div class="home__brand">
            <BrandArt kind="emblem" src="brand/emblem.webp" alt="" />
            <div class="home__words">
              <h1 id="home-title" class="home__title">
                <Wordmark size="home" src="brand/wordmark.svg" />
                <span class="visually-hidden"> · Home</span>
              </h1>
              <p class="home__line">Edit pictures and check the result, with models on your Mac or in the cloud.</p>
              <p class="home__promise mono">cost shown before every run · every run recorded · any run copies as a command</p>
            </div>
          </div>
          <ol class="home__steps">
            {STEPS.map((step) => (
              <li key={step.n} class="home__step">
                <span class="home__step-n mono">{step.n}</span>
                <span class="home__step-title">{step.title}</span>
                <span class="home__step-line">{step.line}</span>
              </li>
            ))}
          </ol>
        </div>
        <div class="home__art">
          <BrandArt kind="crew" src="brand/crew.webp" alt="The whizbang crew at their machine" />
        </div>
      </section>

      <section class="home__section" aria-labelledby="continue-title">
        <h2 id="continue-title" class="section-title">
          Continue
        </h2>
        {open.length === 0 && drafts.length === 0 ? (
          <p class="empty">Nothing is running and there are no drafts. A run you start shows here until it ends.</p>
        ) : (
          <div class="home__continue">
            {open.map((run) => {
              const task = titled(run.action);
              return (
                <ContinueRow
                  key={run.run}
                  title={task?.title ?? run.action}
                  what={`${task?.kit ?? ""} · ${run.run}`}
                  tag={{ kind: "running", detail: run.state === "stopping" ? "stopping" : undefined }}
                  when={ago(run.started_ms, now)}
                  onOpen={() => props.go?.(hrefOf({ name: "run", id: run.run }))}
                />
              );
            })}
            {drafts.map((draft) => {
              const task = titled(draft.task);
              const named = Object.values(draft.pictures)[0]?.name;
              return (
                <ContinueRow
                  key={`draft-${draft.task}`}
                  title={task?.title ?? draft.task}
                  what={named ? `${draft.kit} · ${named}` : draft.kit}
                  tag={{ kind: "draft" }}
                  when={ago(draft.at, now)}
                  onOpen={() => props.go?.(hrefOf({ name: "kit", kit: draft.kit, task: draft.task }))}
                />
              );
            })}
          </div>
        )}
      </section>

      <section class="home__section" aria-labelledby="kits-title">
        <h2 id="kits-title" class="section-title">
          Kits
        </h2>
        {kits.length === 0 ? (
          <p class="empty">This build lists no kits yet.</p>
        ) : (
          <div class="home__kits">
            {kits.map((kit, index) => (
              <KitPanel
                key={kit.id}
                name={kit.name ?? kit.id}
                line={kit.line ?? ""}
                tone={kitTone(index)}
                tools={typeof kit.tools === "number" ? kit.tools : undefined}
                shown={4}
                onMore={() => props.go?.(hrefOf({ name: "kit", kit: kit.id }))}
                tasks={(Array.isArray(kit.tasks) ? kit.tasks : []).map((task) => ({
                  title: task.title ?? task.name,
                  shape: task.shape ?? "",
                  uses: task.uses ? (kits.find((other) => other.id === task.uses)?.name ?? task.uses) : undefined,
                  href: hrefOf({ name: "kit", kit: kit.id, task: task.name }),
                }))}
              />
            ))}
          </div>
        )}
      </section>

      <section class="home__section" aria-labelledby="pictures-title">
        <h2 id="pictures-title" class="section-title">
          Pictures
        </h2>
        {problem ? (
          <p class="field__error" role="alert">
            {problem}
          </p>
        ) : null}
        {pictures.length === 0 && !client ? <p class="empty">No pictures yet. A picture you give a task is kept in the store and shows here.</p> : null}
        {client ? (
          <div class="library home__pictures">
            {pictures.length === 0 ? <p class="empty">No pictures yet. A picture you give a task is kept in the store and shows here.</p> : null}
            {pictures.map((picture) => (
              <StoredPicture key={picture.id} client={client} picture={picture} />
            ))}
            {library.more ? (
              <button type="button" class="link-button" onClick={library.next}>
                More pictures
              </button>
            ) : null}
            <DropTarget label="Drop a picture" max={8} onFiles={upload} />
          </div>
        ) : null}
      </section>
    </main>
  );
}

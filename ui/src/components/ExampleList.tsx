import { useEffect, useState } from "preact/hooks";
import { ApiError, type Client } from "../api";
import { draftOf, localStore, saveDraft } from "../drafts";
import { useArtifact } from "../hooks";
import { hrefOf } from "../routes";
import { deleteExample, draftOfExample, EXAMPLE_NAME_LIMIT, EXAMPLES_LIMIT, listExamples, openExample, renameExample, type Example, type OpenedExample } from "../saved";
import { Button } from "./Button";
import { cx } from "./cx";

export interface ExampleListProps {
  client: Client;
  go: (hash: string) => void;
  /** Only the examples of this kit (the kit workspace); every one on Home. */
  kit?: string;
  /** Kit names by id, for a row's line. */
  kitName?: (kit: string | null | undefined) => string;
}

/** What a row of the list is doing: its menu open, its name being changed, its deletion asked. */
type Doing = { id: string; what: "menu" | "rename" | "delete" } | null;

/**
 * Saved examples (plan m7, task U1, item 2; the design's surface 2, B): on Home and in a
 * kit's workspace, each with its picture, its name, its kit and task, and a menu to open it
 * as a draft, rename it, open its run, or delete it. An example whose task this build no
 * longer has can be read and deleted, not opened. Every name is put in as text.
 */
export function ExampleList(props: ExampleListProps) {
  const [examples, setExamples] = useState<Example[] | null>(null);
  const [asked, setAsked] = useState(0);
  const [doing, setDoing] = useState<Doing>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [replacing, setReplacing] = useState<OpenedExample | null>(null);
  useEffect(() => {
    let live = true;
    listExamples(props.client).then(
      (found) => live && setExamples(found),
      (error: unknown) => live && failed(error),
    );
    return () => {
      live = false;
    };
  }, [asked]);
  // A refused token is the shell's to say, once: the list says nothing of it.
  const failed = (error: unknown) => {
    if (error instanceof ApiError && error.status === 401) return;
    setProblem(error instanceof ApiError ? error.message : String(error));
  };
  const shown = (examples ?? []).filter((one) => !props.kit || one.kit === props.kit);
  const start = (opened: OpenedExample, sure = false) => {
    const store = localStore();
    if (!sure && draftOf(store, opened.task)) {
      setReplacing(opened);
      return;
    }
    saveDraft(store, draftOfExample(opened, Date.now()));
    props.go(hrefOf({ name: "kit", kit: opened.kit ?? "rascal", task: opened.task }));
  };
  const open = (example: Example) => {
    setDoing(null);
    setProblem(null);
    openExample(props.client, example.id).then((opened) => start(opened), failed);
  };
  if (examples !== null && shown.length === 0 && !problem) {
    return (
      <section class="examples" aria-label="Examples">
        <header class="examples__head">
          <h2 class="examples__title">Examples</h2>
          <span class="examples__count mono">{`0 of up to ${EXAMPLES_LIMIT.toLocaleString("en-US")}`}</span>
        </header>
        <p class="empty examples__empty">No examples yet. Save one from a result's More menu: it keeps the task, its settings and its pictures, and opens as a draft.</p>
      </section>
    );
  }
  return (
    <section class="examples" aria-label="Examples">
      <header class="examples__head">
        <h2 class="examples__title">Examples</h2>
        <span class="examples__count mono">{`${shown.length} of up to ${EXAMPLES_LIMIT.toLocaleString("en-US")}`}</span>
      </header>
      {problem ? (
        <p class="field__error examples__problem" role="alert">
          {problem}
        </p>
      ) : null}
      {replacing ? (
        <div class="examples__replacing" role="alert">
          <p class="workspace__purpose">{`This replaces the draft of ${replacing.title} you have not run yet.`}</p>
          <Button variant="secondary" onClick={() => start(replacing, true)}>
            Replace it
          </Button>
          <Button variant="quiet" onClick={() => setReplacing(null)}>
            Keep my draft
          </Button>
        </div>
      ) : null}
      <ul class="examples__list">
        {shown.map((example) => (
          <ExampleRow
            key={example.id}
            client={props.client}
            example={example}
            line={`${props.kitName?.(example.kit) ?? example.kit ?? ""} · ${example.title ?? example.task}`}
            doing={doing?.id === example.id ? doing.what : null}
            onDoing={(what) => setDoing(what ? { id: example.id, what } : null)}
            onOpen={() => open(example)}
            onRun={() => props.go(hrefOf({ name: "run", id: example.run }))}
            onRename={(name) =>
              renameExample(props.client, example.id, name).then(() => {
                setDoing(null);
                setAsked((n) => n + 1);
              }, failed)
            }
            onDelete={() =>
              deleteExample(props.client, example.id).then(() => {
                setDoing(null);
                setAsked((n) => n + 1);
              }, failed)
            }
          />
        ))}
      </ul>
    </section>
  );
}

interface ExampleRowProps {
  client: Client;
  example: Example;
  line: string;
  doing: "menu" | "rename" | "delete" | null;
  onDoing: (what: "menu" | "rename" | "delete" | null) => void;
  onOpen: () => void;
  onRun: () => void;
  onRename: (name: string) => void;
  onDelete: () => void;
}

function ExampleRow(props: ExampleRowProps) {
  const { example } = props;
  const src = useArtifact(props.client, example.picture ?? null, 80);
  const [name, setName] = useState(example.name);
  return (
    <li class={cx("examples__row", !example.known && "examples__row--gone")} data-example={example.id}>
      {src ? <img class="examples__thumb" src={src} alt="" /> : <span class="examples__thumb" aria-hidden="true" />}
      <span class="examples__words">
        {example.known ? (
          <button type="button" class="examples__name" onClick={props.onOpen}>
            {example.name}
          </button>
        ) : (
          <span class="examples__name">{example.name}</span>
        )}
        {example.known ? <span class="examples__line mono">{props.line}</span> : <span class="examples__line examples__line--gone mono">{`this build has no task “${example.task}”`}</span>}
      </span>
      <span class="examples__more">
        <button type="button" class="examples__dots" aria-label={`More for ${example.name}`} aria-expanded={props.doing === "menu" ? "true" : "false"} onClick={() => props.onDoing(props.doing === "menu" ? null : "menu")}>
          ⋯
        </button>
        {props.doing === "menu" ? (
          <div class="menu examples__menu" role="menu" aria-label={`Example ${example.name}`}>
            <button type="button" role="menuitem" class="menu__item" disabled={!example.known} onClick={props.onOpen}>
              <span class="menu__title">Open as draft</span>
            </button>
            <button type="button" role="menuitem" class="menu__item" onClick={() => props.onDoing("rename")}>
              <span class="menu__title">Rename…</span>
            </button>
            <button type="button" role="menuitem" class="menu__item" onClick={props.onRun}>
              <span class="menu__title">Open its run</span>
            </button>
            <span class="menu__rule" aria-hidden="true" />
            <button type="button" role="menuitem" class="menu__item examples__delete" onClick={() => props.onDoing("delete")}>
              <span class="menu__title">Delete example</span>
            </button>
          </div>
        ) : null}
      </span>
      {props.doing === "rename" ? (
        <form
          class="examples__ask"
          onSubmit={(event) => {
            event.preventDefault();
            if (name.trim()) props.onRename(name);
          }}
        >
          <input class="control" type="text" aria-label="New name" value={name} maxLength={EXAMPLE_NAME_LIMIT} onInput={(event) => setName((event.currentTarget as HTMLInputElement).value)} />
          <Button variant="secondary" type="submit" disabled={!name.trim()} why={name.trim() ? undefined : "An example has a name."}>
            Rename
          </Button>
          <Button variant="quiet" onClick={() => props.onDoing(null)}>
            Cancel
          </Button>
        </form>
      ) : null}
      {props.doing === "delete" ? (
        <div class="examples__ask" role="alert">
          <span class="workspace__purpose">{`Delete “${example.name}”? Its run and its pictures stay.`}</span>
          <Button variant="danger" onClick={props.onDelete}>
            Delete
          </Button>
          <Button variant="quiet" onClick={() => props.onDoing(null)}>
            Keep it
          </Button>
        </div>
      ) : null}
    </li>
  );
}

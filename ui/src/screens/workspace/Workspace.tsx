import { useEffect, useMemo, useRef, useState } from "preact/hooks";
import { ApiError, backendsOf, tasksOf, type Client, type Hello, type OpenRun, type Picture, type Prepared, type Task, type TaskField } from "../../api";
import { BindingPicker } from "../../components/BindingPicker";
import { Button } from "../../components/Button";
import { Canvas, type DrawTool, type Mark } from "../../components/Canvas";
import { ChoiceList } from "../../components/ChoiceList";
import { ContinueRow } from "../../components/ContinueRow";
import { DropTarget } from "../../components/DropTarget";
import { Field } from "../../components/Field";
import { KitPanel } from "../../components/KitPanel";
import { kitTone } from "../../components/kitTone";
import { ListField } from "../../components/ListField";
import { NumberField } from "../../components/NumberField";
import { PictureField } from "../../components/PictureField";
import { PlanBlock } from "../../components/PlanBlock";
import { Segmented } from "../../components/Segmented";
import { TextField } from "../../components/TextField";
import { dropDraft, draftOf, localStore, saveDraft } from "../../drafts";
import { argumentsOf, choicesFor, defaultChoice, fieldsOf, given, labelOf, planNeeds, segmentQuery, shapeOf, whyNotRun, type Values } from "../../form";
import { ago, dollars } from "../../format";
import { useArtifact } from "../../hooks";
import { actionOf } from "../../keyboard";
import { seenOutputs } from "../../outputs";
import { createPreparer, type Preparer } from "../../preparer";
import { hrefOf } from "../../routes";
import { EmptyScreen } from "../Empty";
import { Library } from "../shared";

export interface WorkspaceProps {
  kit: string;
  task?: string;
  hello?: Hello | null;
  client?: Client;
  go?: (hash: string) => void;
}

/** The kit workspace: a task's draft, its inputs, its plan and Run (plan m5, task W1). */
export function Workspace(props: WorkspaceProps) {
  const kits = Array.isArray(props.hello?.kits) ? props.hello.kits : [];
  const tasks = tasksOf(props.hello);
  const task = props.task ? tasks.find((one) => one.name === props.task) : undefined;
  if (!props.task) {
    const index = kits.findIndex((kit) => kit.id === props.kit);
    const kit = kits[index];
    if (!kit) {
      return (
        <EmptyScreen title={props.kit} link={{ href: "#/", words: "Back to Home" }}>
          This build has no kit of that name.
        </EmptyScreen>
      );
    }
    return (
      <main class="screen" aria-labelledby="screen-title">
        <h1 id="screen-title" class="screen__title">
          {kit.name ?? kit.id}
        </h1>
        <KitPanel
          name={kit.name ?? kit.id}
          line={kit.line ?? ""}
          tone={kitTone(index)}
          tools={typeof kit.tools === "number" ? kit.tools : undefined}
          tasks={(Array.isArray(kit.tasks) ? kit.tasks : []).map((one) => ({
            title: one.title ?? one.name,
            shape: one.shape ?? "",
            uses: one.uses ? (kits.find((other) => other.id === one.uses)?.name ?? one.uses) : undefined,
            href: hrefOf({ name: "kit", kit: kit.id, task: one.name }),
          }))}
        />
      </main>
    );
  }
  if (!task || !props.client) {
    return (
      <EmptyScreen title={props.task} link={{ href: "#/", words: "Back to Home" }}>
        This build has no task of that name.
      </EmptyScreen>
    );
  }
  return <Draft key={task.name} task={task} kit={props.kit} hello={props.hello ?? null} client={props.client} go={props.go ?? (() => undefined)} />;
}

interface DraftProps {
  task: Task;
  kit: string;
  hello: Hello | null;
  client: Client;
  go: (hash: string) => void;
}

/** The field whose picture the canvas shows: the one a drawn field is on, else the first picture. */
function canvasField(fields: TaskField[]): TaskField | undefined {
  const named = fields.find((field) => field.on)?.on;
  return fields.find((field) => field.name === named) ?? fields.find((field) => field.kind === "picture") ?? fields.find((field) => field.kind === "pictures");
}

interface Tool {
  value: string;
  label: string;
  field: TaskField;
  tool: DrawTool;
}

/** The canvas tools a task offers: each location's, and segment's point and box. */
function toolsOf(fields: TaskField[]): Tool[] {
  const tools: Tool[] = [];
  for (const field of fields) {
    if (field.canvas === "point_or_box") {
      tools.push({ value: `${field.name}|point`, label: "Point", field, tool: "point" });
      tools.push({ value: `${field.name}|box`, label: "Box", field, tool: "box" });
      continue;
    }
    if (field.kind !== "artifact" || field.artifact !== "location") continue;
    const draw = field.draw ?? [];
    if (draw.includes("box") || draw.includes("polygon")) tools.push({ value: `${field.name}|box`, label: `${labelOf(field)} · box`, field, tool: "box" });
    if (draw.includes("stroke") || draw.includes("line")) tools.push({ value: `${field.name}|stroke`, label: `${labelOf(field)} · ${draw.includes("stroke") ? "stroke" : "line"}`, field, tool: "stroke" });
  }
  return tools;
}

/** The first picture a field's value names. */
function firstId(value: unknown): string | null {
  if (typeof value === "string") return value;
  if (Array.isArray(value) && typeof value[0] === "string") return value[0];
  return null;
}

function Draft(props: DraftProps) {
  const { task, client } = props;
  const fields = fieldsOf(task);
  const backends = backendsOf(props.hello);
  const store = localStore();
  const kept = useMemo(() => draftOf(store, task.name), [task.name]);
  const [values, setValues] = useState<Values>(kept?.values ?? {});
  const [pictures, setPictures] = useState<Record<string, Picture>>(kept?.pictures ?? {});
  const [library, setLibrary] = useState<Picture[] | null>(null);
  const [picking, setPicking] = useState<string | null>(null);
  const [prepared, setPrepared] = useState<Prepared | null>(null);
  const [preparing, setPreparing] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  const [drawn, setDrawn] = useState<Record<string, Mark>>({});
  const tools = toolsOf(fields);
  const [tool, setTool] = useState<string>(tools[0]?.value ?? "");
  const [zoom, setZoom] = useState<number | "fit">("fit");
  const [starting, setStarting] = useState(false);
  const [open, setOpen] = useState<OpenRun[]>([]);
  const preparer = useRef<Preparer | null>(null);

  const set = (name: string, value: unknown) => setValues((now) => ({ ...now, [name]: value }));
  const args = argumentsOf(task, values);
  const argsKey = JSON.stringify(args);

  useEffect(() => {
    const made = createPreparer({
      prepare: (action, sent) => client.prepare(action, sent),
      onAnswer: (answer) => setPrepared(answer),
      onError: (error) => setProblem(error instanceof ApiError ? error.message : String(error)),
      onBusy: setPreparing,
    });
    preparer.current = made;
    return () => made.stop();
  }, [client]);

  useEffect(() => {
    preparer.current?.change(task.name, args);
    if (Object.keys(args).length > 0) saveDraft(store, { kit: props.kit, task: task.name, values, pictures, at: Date.now() });
  }, [argsKey]);

  useEffect(() => {
    let live = true;
    client.openRuns().then(
      (runs) => live && setOpen(runs.filter((run) => run.action === task.name)),
      () => undefined,
    );
    return () => {
      live = false;
    };
  }, [task.name]);

  const openLibrary = (name: string) => {
    setPicking((now) => (now === name ? null : name));
    client.listPictures().then(setLibrary, () => setLibrary([]));
  };

  const keep = (picture: Picture) => setPictures((now) => ({ ...now, [picture.id]: picture }));
  const upload = async (files: File[]): Promise<Picture[]> => {
    setProblem(null);
    const made: Picture[] = [];
    for (const file of files) {
      try {
        const picture = await client.uploadPicture(file);
        keep(picture);
        made.push(picture);
      } catch (error) {
        setProblem(error instanceof ApiError ? error.message : String(error));
      }
    }
    return made;
  };

  const shown = canvasField(fields);
  const shownId = shown ? firstId(values[shown.name]) : null;
  const shownPicture = shownId ? pictures[shownId] : undefined;
  const shownSrc = useArtifact(client, shownId);
  const current = tools.find((one) => one.value === tool) ?? tools[0];

  const onDraw = (mark: Mark) => {
    if (!current) return;
    const field = current.field;
    if (field.canvas === "point_or_box") {
      const query = segmentQuery(mark);
      if (!query) return;
      setValues((now) => ({ ...now, [field.name]: query.query, ...(fields.some((one) => one.name === "kind") ? { kind: query.kind } : {}) }));
      setDrawn((now) => ({ ...now, [field.name]: mark }));
      return;
    }
    const shape = shapeOf(mark, field.draw);
    const picture = firstId(values[field.on ?? shown?.name ?? ""]);
    if (!shape || !picture) return;
    setProblem(null);
    client.commitLocation(picture, shape).then(
      (made) => {
        set(field.name, made.id);
        setDrawn((now) => ({ ...now, [field.name]: mark.kind === "box" ? { ...mark, label: labelOf(field) } : mark }));
      },
      (error: unknown) => setProblem(error instanceof ApiError ? error.message : String(error)),
    );
  };

  const why = whyNotRun(task, values, prepared, preparing);
  const bound = prepared?.bound ?? 0;
  const run = () => {
    if (why !== null || starting || !prepared?.handle) return;
    setStarting(true);
    setProblem(null);
    client.start({ action: task.name, arguments: args, handle: prepared.handle }).then(
      (started) => {
        dropDraft(store, task.name);
        props.go(hrefOf({ name: "run", id: started.run }));
      },
      (error: unknown) => {
        setStarting(false);
        if (error instanceof ApiError && error.status === 409 && error.kind === "stale") {
          setPrepared(null);
          setProblem(`${error.message} The call was prepared again: look at the plan and press Run once more.`);
          preparer.current?.now(task.name, args);
          return;
        }
        setProblem(error instanceof ApiError ? error.message : String(error));
      },
    );
  };
  const runRef = useRef(run);
  runRef.current = run;
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (actionOf(event)?.kind !== "run") return;
      event.preventDefault();
      runRef.current();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const marks = Object.values(drawn);
  return (
    <main class="workspace" aria-labelledby="task-title">
      <section class="workspace__left">
        <header class="workspace__head">
          <h1 id="task-title" class="workspace__title">
            {task.title ?? task.name}
          </h1>
          {task.purpose ? <p class="workspace__purpose">{task.purpose}</p> : null}
        </header>
        <div class="workspace__fields">
          {fields.map((field) => (
            <Input
              key={field.name}
              field={field}
              value={values[field.name]}
              backends={backends}
              prepared={prepared}
              pictures={pictures}
              client={client}
              picking={picking === field.name}
              library={library}
              drawn={Boolean(drawn[field.name])}
              onChange={(value) => set(field.name, value)}
              onClear={() => {
                set(field.name, undefined);
                setDrawn((now) => {
                  const next = { ...now };
                  delete next[field.name];
                  return next;
                });
              }}
              onUpload={upload}
              onLibrary={() => openLibrary(field.name)}
              onPicked={(picture) => {
                keep(picture);
                setPicking(null);
              }}
            />
          ))}
        </div>
        <footer class="workspace__foot">
          <PlanBlock needs={planNeeds(prepared, backendsOf(props.hello))} bound={bound} attempts={prepared?.attempts ?? undefined} stale={preparing} />
          {problem ? (
            <p class="field__error" role="alert">
              {problem}
            </p>
          ) : null}
          <div class="workspace__run">
            <span class="workspace__bound mono">{prepared?.ready ? (bound > 0 ? `up to ${dollars(bound)}` : "free") : ""}</span>
            <Button variant="primary" hint="⌘↵" disabled={why !== null || starting} why={why ?? undefined} busy={starting} onClick={run}>
              Run
            </Button>
          </div>
        </footer>
      </section>
      <section class="workspace__centre" aria-label="Canvas">
        <div class="workspace__tabs">
          <Segmented label="View" segments={[{ value: "original", label: "Original" }]} value="original" />
          {tools.length > 0 ? <Segmented label="Draw" segments={tools.map((one) => ({ value: one.value, label: one.label }))} value={current?.value ?? ""} onChange={setTool} /> : null}
        </div>
        {shownId && shownSrc && shownPicture?.width && shownPicture.height ? (
          <Canvas src={shownSrc} name={shownPicture.name} width={shownPicture.width} height={shownPicture.height} marks={marks} zoom={zoom} tool={current?.tool ?? null} onDraw={onDraw} onZoom={setZoom} />
        ) : (
          <p class="empty workspace__nothing">{shown ? "Give the task a picture to see it here." : "This task takes no picture."}</p>
        )}
      </section>
      <aside class="workspace__right">
        <h2 class="section-title">Runs of this task</h2>
        {open.length === 0 ? (
          <p class="empty">Not run yet here. The plan on the left is what will happen.</p>
        ) : (
          open.map((one) => (
            <ContinueRow key={one.run} title={task.title ?? task.name} what={one.run} tag={{ kind: "running" }} when={ago(one.started_ms, Date.now())} onOpen={() => props.go(hrefOf({ name: "run", id: one.run }))} />
          ))
        )}
        <h2 class="section-title">Tools this task uses</h2>
        <p class="workspace__tools mono">{Array.isArray(task.tools) && task.tools.length > 0 ? task.tools.join(" · ") : "none"}</p>
      </aside>
    </main>
  );
}

interface InputProps {
  field: TaskField;
  value: unknown;
  backends: ReturnType<typeof backendsOf>;
  prepared: Prepared | null;
  pictures: Record<string, Picture>;
  client: Client;
  picking: boolean;
  library: Picture[] | null;
  drawn: boolean;
  onChange: (value: unknown) => void;
  onClear: () => void;
  onUpload: (files: File[]) => Promise<Picture[]>;
  onLibrary: () => void;
  onPicked: (picture: Picture) => void;
}

/** One field of the form, by its kind. */
function Input(props: InputProps) {
  const { field } = props;
  const id = `field-${field.name}`;
  const label = labelOf(field);
  const optional = !field.required;
  const help = field.purpose;
  switch (field.kind) {
    case "text":
      return <TextField id={id} label={label} value={typeof props.value === "string" ? props.value : ""} optional={optional} help={help} onInput={props.onChange} />;
    case "number":
    case "integer": {
      const integer = field.kind === "integer";
      const min = field.min ?? undefined;
      const max = field.max ?? undefined;
      if (integer && min !== undefined && max !== undefined && max - min <= 8) {
        const shown = typeof props.value === "number" ? props.value : typeof field.default === "number" ? field.default : min;
        const segments = Array.from({ length: max - min + 1 }, (_, at) => ({ value: String(min + at), label: String(min + at) }));
        return (
          <Field id={id} label={label} optional={optional} help={help}>
            <Segmented label={label} look="numbers" segments={segments} value={String(shown)} onChange={(next) => props.onChange(Number(next))} />
          </Field>
        );
      }
      return <NumberField id={id} label={label} value={typeof props.value === "number" ? props.value : null} integer={integer} min={min} max={max} optional={optional} help={help} onInput={props.onChange} />;
    }
    case "boolean": {
      const shown = typeof props.value === "boolean" ? props.value : field.default === true;
      return (
        <Field id={id} label={label} optional={optional} help={help}>
          <Segmented
            label={label}
            segments={[
              { value: "yes", label: "Yes" },
              { value: "no", label: "No" },
            ]}
            value={shown ? "yes" : "no"}
            onChange={(next) => props.onChange(next === "yes")}
          />
        </Field>
      );
    }
    case "choice": {
      const choices = Array.isArray(field.choices) ? field.choices : [];
      const shown = typeof props.value === "string" ? props.value : typeof field.default === "string" ? field.default : (choices[0] ?? "");
      return (
        <Field id={id} label={label} optional={optional} help={help}>
          <Segmented label={label} segments={choices.map((choice) => ({ value: choice, label: choice }))} value={shown} onChange={props.onChange} />
        </Field>
      );
    }
    case "backend": {
      const choices = choicesFor(field, props.prepared, props.backends);
      const shown = typeof props.value === "string" ? props.value : defaultChoice(field, props.backends);
      return (
        <Field id={id} label={label} optional={optional} help={choices.length === 0 ? "The config offers no backend for this." : help}>
          <ChoiceList label={label} name={id} choices={choices} value={shown} onChange={props.onChange} />
        </Field>
      );
    }
    case "picture":
      return <PictureInput {...props} id={id} label={label} />;
    case "pictures":
      return <PicturesInput {...props} id={id} label={label} />;
    case "artifact": {
      const drawable = field.artifact === "location" && Array.isArray(field.draw) && field.draw.length > 0;
      return (
        <Field id={id} label={label} optional={optional} help={help}>
          {drawable ? (
            <div class="workspace__drawn">
              <span class="mono">{props.drawn || given(props.value) ? "drawn on the picture" : `draw a ${(field.draw ?? []).join(" or ")} on the picture`}</span>
              {given(props.value) ? (
                <button type="button" class="link-button" onClick={props.onClear}>
                  Clear
                </button>
              ) : null}
            </div>
          ) : null}
          <BindingPicker type={field.artifact ?? ""} bindings={seenOutputs()} value={typeof props.value === "string" ? props.value : null} onPick={props.onChange} />
        </Field>
      );
    }
  }
}

function PictureInput(props: InputProps & { id: string; label: string }) {
  const id = typeof props.value === "string" ? props.value : null;
  const known = id ? props.pictures[id] : undefined;
  const src = useArtifact(props.client, id, 256);
  return (
    <div class="workspace__picture">
      <PictureField
        id={props.id}
        label={props.label}
        optional={!props.field.required}
        help={props.field.purpose}
        picture={id ? { id, src: src ?? "", name: known?.name ?? id, width: known?.width, height: known?.height } : null}
        onDrop={(files) => {
          void props.onUpload(files.slice(0, 1)).then((made) => {
            if (made[0]) props.onChange(made[0].id);
          });
        }}
        onChoose={props.onLibrary}
        onClear={props.onClear}
      />
      {props.picking && !id ? (
        <Library
          client={props.client}
          pictures={props.library ?? []}
          label={`Pictures for ${props.label}`}
          onPick={(picture) => {
            props.onPicked(picture);
            props.onChange(picture.id);
          }}
        />
      ) : null}
    </div>
  );
}

function PicturesInput(props: InputProps & { id: string; label: string }) {
  const ids = Array.isArray(props.value) ? props.value.filter((one): one is string => typeof one === "string") : [];
  const min = props.field.count?.min ?? (props.field.required ? 1 : 0);
  const max = props.field.count?.max ?? null;
  const room = max === null ? 16 : Math.max(0, max - ids.length);
  const add = (made: Picture[]) => props.onChange([...ids, ...made.map((picture) => picture.id)].slice(0, max ?? undefined));
  return (
    <div class="workspace__picture">
      <ListField
        id={props.id}
        label={props.label}
        min={min}
        max={max}
        items={ids.map((one, index) => ({ key: `${index}-${one}`, label: props.pictures[one]?.name ?? one }))}
        onMove={(from, to) => {
          const next = [...ids];
          const [moved] = next.splice(from, 1);
          if (moved !== undefined) next.splice(to, 0, moved);
          props.onChange(next);
        }}
        onRemove={(index) => props.onChange(ids.filter((_, at) => at !== index))}
        adder={
          <div class="workspace__adder">
            <DropTarget label="Drop pictures" max={room} onFiles={(files) => void props.onUpload(files.slice(0, room)).then(add)} />
            <button type="button" class="link-button" onClick={props.onLibrary}>
              From the library
            </button>
          </div>
        }
      />
      {props.picking && room > 0 ? (
        <Library
          client={props.client}
          pictures={props.library ?? []}
          label={`Pictures for ${props.label}`}
          onPick={(picture) => {
            props.onPicked(picture);
            add([picture]);
          }}
        />
      ) : null}
    </div>
  );
}

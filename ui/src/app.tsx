import { useEffect, useMemo, useRef, useState } from "preact/hooks";
import { ApiError, backendsOf, tasksOf, type Client, type Hello, type Picture } from "./api";
import { Breadcrumb } from "./components/Breadcrumb";
import { KitSwitcher } from "./components/KitSwitcher";
import { QuitDialog } from "./components/QuitDialog";
import { kitTone } from "./components/kitTone";
import { SearchPalette, type SearchEntry } from "./components/SearchPalette";
import { StatusBar } from "./components/StatusBar";
import { TopBar } from "./components/TopBar";
import { actionOf } from "./keyboard";
import { hrefOf, parseRoute, type Route } from "./routes";
import { EmptyScreen } from "./screens/Empty";
import type { Taken } from "./token";
import { History } from "./screens/history/History";
import { Home } from "./screens/homepage/Home";
import { Models } from "./screens/models/Models";
import { Record } from "./screens/record/Record";
import { Report } from "./screens/report/Report";
import { Run } from "./screens/run/Run";
import { Workspace } from "./screens/workspace/Workspace";

export interface AppProps {
  client: Client;
  /** The launch token was found (in the address, or kept from before). */
  hasToken: boolean;
  /** The address's hash and a way to follow it; the page passes `location`. */
  initialHash: string;
  subscribeHash: (listener: (hash: string) => void) => () => void;
  /**
   * The address's hash now. The page follows hash changes from when it subscribes, after
   * its first paint; a change before then (a link followed, or an address typed, while the
   * page was still starting) is caught up with this once subscribed. The page passes
   * `location.hash`.
   */
  hashNow?: () => string;
  go: (hash: string) => void;
  /**
   * Reads a `#k=` the address carries now, takes it out of the bar and keeps it
   * (`takeToken`); the page calls it on every hash change, so a token handed to a tab that
   * is already open is used, and an address that tries to hand one over is never left.
   */
  takeToken: () => Taken;
}

/** The outlined wordmark, the owner's art (plan m5, task W3, item 6). */
export const WORDMARK = "brand/wordmark.svg";

/** The sentence when the page holds no token at all. */
export const NO_TOKEN = "This page was opened without its key. Run whizbang play again, which opens the page with a new one.";

/** What the top bar shows of hello's allowance: spent of the allowance, or none set. */
type Allowance = { allowance: number; spent: number; held: number; exactly: boolean } | null;

/** The allowance of hello's answer: spent and held since the last reset, or none set. */
export function allowanceOf(hello: Hello): Allowance {
  const said = hello.allowance;
  if (!said || typeof said.allowance !== "number") return null;
  const spent = typeof said.spent === "number" ? said.spent : typeof said.left === "number" ? Math.max(0, said.allowance - said.left) : 0;
  return { allowance: said.allowance, spent, held: typeof said.held === "number" ? said.held : 0, exactly: said.exactly !== false };
}

/** The shell: the top bar, the screen of the route, the status bar, search and the keys. */
export function App(props: AppProps) {
  const [hello, setHello] = useState<Hello | null>(null);
  const [problem, setProblem] = useState<string | null>(props.hasToken ? null : NO_TOKEN);
  const [route, setRoute] = useState<Route>(parseRoute(props.initialHash));
  const [searching, setSearching] = useState(false);
  const [switching, setSwitching] = useState(false);
  // Quit: asked first, with the runs going counted; then the host is told, and the page
  // says it has quit and asks nothing more.
  const [quit, setQuit] = useState<{ stage: "asking" | "quitting" | "quit"; going: number | null; problem?: string } | null>(null);
  const askQuit = () => {
    setQuit({ stage: "asking", going: null });
    props.client.openRuns().then(
      (runs) => setQuit((now) => (now?.stage === "asking" ? { ...now, going: runs.length } : now)),
      () => undefined,
    );
  };
  const doQuit = () => {
    setQuit((now) => ({ stage: "quitting", going: now?.going ?? null }));
    const quitted = () => setQuit((now) => ({ stage: "quit", going: now?.going ?? null }));
    const failed = (error: unknown) => setQuit((now) => ({ stage: "asking", going: now?.going ?? null, problem: error instanceof ApiError ? error.message : String(error) }));
    props.client.quit().then(quitted, (error: unknown) => {
      // No answer at all: the host may have stopped before its answer reached the page.
      // It has quit when it no longer answers hello either (or answers that it is stopping).
      if (!(error instanceof ApiError) || error.status !== 0) return failed(error);
      props.client.hello().then(
        () => failed(error),
        (again: unknown) => (again instanceof ApiError && (again.status === 0 || again.status === 503) ? quitted() : failed(error)),
      );
    });
  };

  const [asked, setAsked] = useState(0);
  useEffect(() => {
    const follow = (hash: string) => {
      const taken = props.takeToken();
      if (taken.fresh) {
        props.client.setToken(taken.token);
        setProblem(null);
        setAsked((n) => n + 1);
      }
      setRoute(parseRoute(taken.offered ? "#/" : hash));
    };
    const stop = props.subscribeHash(follow);
    // A change made between the first render and now fired no event this page heard.
    const now = props.hashNow?.();
    if (now !== undefined && now !== props.initialHash) follow(now);
    return stop;
  }, []);
  // The top bar's allowance: from the first hello, then read again from hello whenever
  // the books may have changed (a run the page followed ended, a run going is no longer
  // listed, a reset). Every answer is numbered, the first among them, and only the newest
  // asked is taken, so a slow first answer never puts back an older figure.
  const [allowance, setAllowance] = useState<Allowance | undefined>(undefined);
  const asking = useRef(0);
  useEffect(() => {
    let live = true;
    const mine = (asking.current += 1);
    props.client.hello().then(
      (answer) => {
        if (!live) return;
        setHello(answer);
        if (mine === asking.current) setAllowance(allowanceOf(answer));
      },
      (error: unknown) => live && setProblem(error instanceof ApiError ? error.message : String(error)),
    );
    return () => {
      live = false;
    };
  }, [asked]);
  useEffect(() => {
    if (typeof props.client.onSettled !== "function") return;
    const stop = props.client.onSettled(() => {
      const mine = (asking.current += 1);
      props.client.hello().then(
        (answer) => mine === asking.current && setAllowance(allowanceOf(answer)),
        () => undefined,
      );
    });
    return stop;
  }, []);

  const kits = Array.isArray(hello?.kits) ? hello.kits : [];
  const [pictures, setPictures] = useState<Picture[]>([]);
  useEffect(() => {
    if (!searching || typeof props.client.listPictures !== "function") return;
    let live = true;
    props.client.listPictures().then(
      (found) => live && setPictures(found),
      () => undefined,
    );
    return () => {
      live = false;
    };
  }, [searching]);
  const entries = useMemo<SearchEntry[]>(() => {
    const kitName = (id: string | null | undefined) => kits.find((kit) => kit.id === id)?.name ?? id ?? "";
    const tasks = tasksOf(hello).map((task) => ({
      key: hrefOf({ name: "kit", kit: task.kit ?? "tools", task: task.name }),
      kind: task.kind === "primitive" ? ("tool" as const) : ("task" as const),
      label: task.title ?? task.name,
      detail: task.kind === "primitive" ? `tool${task.kit ? ` · ${kitName(task.kit)}` : ""}` : kitName(task.kit),
    }));
    const shown = pictures.map((picture) => ({ key: hrefOf({ name: "home", picture: picture.id }), kind: "picture" as const, label: picture.name, detail: "picture" }));
    // Each task's and tool's record (plan m5, task W3, item 7).
    const records = tasksOf(hello).map((task) => ({
      key: hrefOf({ name: "record", record: task.name }),
      kind: "tool" as const,
      label: `record of ${task.name}`,
      detail: "what it does, its examples and limits",
    }));
    return [...tasks, ...shown, ...records];
  }, [hello, pictures]);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      const action = actionOf(event);
      if (!action) return;
      if (action.kind === "search") setSearching(true);
      else if (action.kind === "home") props.go(hrefOf({ name: "home" }));
      else if (action.kind === "kit") {
        const kit = kits[action.index];
        if (!kit) return;
        props.go(hrefOf({ name: "kit", kit: kit.id }));
      } else return;
      event.preventDefault();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [hello]);

  const kitIndex = route.name === "kit" ? kits.findIndex((kit) => kit.id === route.kit) : -1;
  const where =
    route.name === "kit" ? (
      <Breadcrumb
        kit={kits[kitIndex]?.name ?? route.kit}
        tone={kitTone(Math.max(0, kitIndex))}
        task={route.task ? (tasksOf(hello).find((task) => task.name === route.task)?.title ?? route.task) : undefined}
        onKit={() => setSwitching((open) => !open)}
        onSearch={() => setSearching(true)}
      />
    ) : route.name === "home" ? (
      <span class="top-bar__place">Home</span>
    ) : null;

  if (quit?.stage === "quit") {
    return (
      <div class="shell">
        <div class="shell__screen">
          <EmptyScreen title="The playground has quit">
            {`The host has stopped. ${quit.going ? `It cancelled ${quit.going === 1 ? "the run" : `the ${quit.going} runs`} still going; each one's record says cancelled. ` : ""}Run whizbang play to open the playground again.`}
          </EmptyScreen>
        </div>
      </div>
    );
  }

  return (
    <div class="shell">
      <TopBar
        where={where}
        spent={allowance?.spent}
        held={allowance?.held}
        exactly={allowance?.exactly}
        allowance={allowance === undefined ? undefined : (allowance?.allowance ?? null)}
        current={route.name === "history" ? "history" : route.name === "models" ? "models" : undefined}
        homeHref={hrefOf({ name: "home" })}
        wordmark={WORDMARK}
        historyHref={hrefOf({ name: "history", query: new URLSearchParams() })}
        modelsHref={hrefOf({ name: "models" })}
        onSearch={() => setSearching(true)}
        onQuit={hello ? askQuit : undefined}
      />
      {switching ? (
        <div class="shell__switcher">
          <KitSwitcher
            kits={kits.map((kit, index) => ({ id: kit.id, name: kit.name ?? kit.id, line: kit.line ?? "", tone: kitTone(index) }))}
            current={route.name === "kit" ? route.kit : undefined}
            onPick={(id) => {
              setSwitching(false);
              props.go(hrefOf({ name: "kit", kit: id }));
            }}
            onHome={() => {
              setSwitching(false);
              props.go(hrefOf({ name: "home" }));
            }}
          />
        </div>
      ) : null}
      {problem ? (
        <div class="shell__problem" role="alert">
          {problem}
        </div>
      ) : null}
      <div class="shell__screen">{screenOf(route, hello, props.client, props.go)}</div>
      <StatusBar
        ready={hello !== null && problem === null}
        store={hello?.store}
        backends={[
          ...backendsOf(hello).map((backend) => ({ name: backend.name, ready: backend.ready, why: backend.ready ? undefined : (backend.missing ?? "not ready").replace(/_/g, " ") })),
          ...(typeof hello?.backends_error === "string" ? [{ name: "backends", ready: false, why: hello.backends_error }] : []),
        ]}
        version={hello?.version}
        problem={problem ?? undefined}
      />
      {quit ? (
        <QuitDialog going={quit.going} quitting={quit.stage === "quitting"} problem={quit.problem} onQuit={doQuit} onCancel={() => quit.stage === "asking" && setQuit(null)} />
      ) : null}
      {searching ? (
        <SearchPalette
          entries={entries}
          onClose={() => setSearching(false)}
          onPick={(key) => {
            setSearching(false);
            props.go(key);
          }}
        />
      ) : null}
    </div>
  );
}

function screenOf(route: Route, hello: Hello | null, client: Client, go: (hash: string) => void) {
  switch (route.name) {
    case "home":
      return <Home hello={hello} client={client} go={go} picture={route.picture} />;
    case "kit":
      return <Workspace kit={route.kit} task={route.task} hello={hello} client={client} go={go} />;
    case "run":
      return <Run id={route.id} hello={hello} client={client} go={go} />;
    case "history":
      return <History hello={hello} client={client} query={route.query} go={go} />;
    case "report":
      return <Report id={route.id} hello={hello} client={client} />;
    case "record":
      return <Record name={route.record} hello={hello} client={client} />;
    case "models":
      return <Models client={client} />;
    case "unknown":
      return (
        <EmptyScreen title="Not a page" link={{ href: "#/", words: "Back to Home" }}>
          The playground has no page at this address.
        </EmptyScreen>
      );
  }
}

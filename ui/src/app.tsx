import { useEffect, useMemo, useState } from "preact/hooks";
import { ApiError, type Client, type Hello } from "./api";
import { Breadcrumb } from "./components/Breadcrumb";
import { KitSwitcher } from "./components/KitSwitcher";
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
import { Run } from "./screens/run/Run";
import { Workspace } from "./screens/workspace/Workspace";

export interface AppProps {
  client: Client;
  /** The launch token was found (in the address, or kept from before). */
  hasToken: boolean;
  /** The address's hash and a way to follow it; the page passes `location`. */
  initialHash: string;
  subscribeHash: (listener: (hash: string) => void) => () => void;
  go: (hash: string) => void;
  /**
   * Reads a `#k=` the address carries now, takes it out of the bar and keeps it
   * (`takeToken`); the page calls it on every hash change, so a token handed to a tab that
   * is already open is used, and an address that tries to hand one over is never left.
   */
  takeToken: () => Taken;
}

/** The sentence when the page holds no token at all. */
export const NO_TOKEN = "This page was opened without its key. Run whizbang play again, which opens the page with a new one.";

/** The shell: the top bar, the screen of the route, the status bar, search and the keys. */
export function App(props: AppProps) {
  const [hello, setHello] = useState<Hello | null>(null);
  const [problem, setProblem] = useState<string | null>(props.hasToken ? null : NO_TOKEN);
  const [route, setRoute] = useState<Route>(parseRoute(props.initialHash));
  const [searching, setSearching] = useState(false);
  const [switching, setSwitching] = useState(false);

  const [asked, setAsked] = useState(0);
  useEffect(
    () =>
      props.subscribeHash((hash) => {
        const taken = props.takeToken();
        if (taken.fresh) {
          props.client.setToken(taken.token);
          setProblem(null);
          setAsked((n) => n + 1);
        }
        setRoute(parseRoute(taken.offered ? "#/" : hash));
      }),
    [],
  );
  useEffect(() => {
    let live = true;
    props.client.hello().then(
      (answer) => live && setHello(answer),
      (error: unknown) => live && setProblem(error instanceof ApiError ? error.message : String(error)),
    );
    return () => {
      live = false;
    };
  }, [asked]);

  const kits = hello?.kits ?? [];
  const entries = useMemo<SearchEntry[]>(
    () =>
      kits.flatMap((kit) =>
        (kit.tasks ?? []).map((task) => ({ key: hrefOf({ name: "kit", kit: kit.id, task: task.name }), kind: "task" as const, label: task.title ?? task.name, detail: kit.name ?? kit.id })),
      ),
    [hello],
  );

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
        task={route.task}
        onKit={() => setSwitching((open) => !open)}
        onSearch={() => setSearching(true)}
      />
    ) : route.name === "home" ? (
      <span class="top-bar__place">Home</span>
    ) : null;

  return (
    <div class="shell">
      <TopBar
        where={where}
        left={hello?.allowance?.left}
        allowance={hello?.allowance?.allowance}
        current={route.name === "history" ? "history" : route.name === "models" ? "models" : undefined}
        homeHref={hrefOf({ name: "home" })}
        historyHref={hrefOf({ name: "history", query: new URLSearchParams() })}
        modelsHref={hrefOf({ name: "models" })}
        onSearch={() => setSearching(true)}
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
      <div class="shell__screen">{screenOf(route, hello)}</div>
      <StatusBar
        ready={hello !== null && problem === null}
        store={hello?.store}
        backends={(hello?.backends ?? []).map((backend) => ({ name: backend.name, ready: backend.ready, why: backend.why }))}
        version={hello?.version}
        problem={problem ?? undefined}
      />
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

function screenOf(route: Route, hello: Hello | null) {
  switch (route.name) {
    case "home":
      return <Home hello={hello} />;
    case "kit":
      return <Workspace kit={route.kit} task={route.task} />;
    case "run":
      return <Run id={route.id} />;
    case "history":
      return <History />;
    case "models":
      return <Models />;
    case "unknown":
      return (
        <EmptyScreen title="Not a page" link={{ href: "#/", words: "Back to Home" }}>
          The playground has no page at this address.
        </EmptyScreen>
      );
  }
}

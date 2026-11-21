export interface BackendState {
  /** The backend's short name, as shown: "SAM3", "FAL". */
  name: string;
  ready: boolean;
  /** Why it is not ready, in a word or two: "no key". */
  why?: string;
}

export interface StatusBarProps {
  /** The host answered and its store is held. */
  ready: boolean;
  /** The store's folder, as the host names it. */
  store?: string;
  backends: BackendState[];
  version?: string;
  /** When the host cannot be reached or refused: the sentence to show instead. */
  problem?: string;
}

/** The bottom line: the host's state, the store, each backend's readiness, the version. */
export function StatusBar(props: StatusBarProps) {
  return (
    <footer class="status-bar mono" role="status">
      <span class="status-bar__left">
        {props.problem ? (
          <span class="tone--lacquer">{props.problem}</span>
        ) : (
          <>
            <span class={props.ready ? "tone--pass" : "tone--lacquer"} aria-hidden="true">
              ●
            </span>{" "}
            {props.ready ? "ready" : "not ready"}
            {props.store ? <> · store {props.store}</> : null}
            {props.backends.map((backend) => (
              <span key={backend.name} class="status-bar__backend">
                {" · "}
                {backend.name}{" "}
                {backend.ready ? (
                  <span class="tone--pass" aria-label="ready">
                    ●
                  </span>
                ) : (
                  <span class="tone--lacquer">{backend.why ?? "not ready"}</span>
                )}
              </span>
            ))}
          </>
        )}
      </span>
      {props.version ? <span class="status-bar__version">whizbang {props.version}</span> : null}
    </footer>
  );
}

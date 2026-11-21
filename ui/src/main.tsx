import { render } from "preact";
import { createClient } from "./api";
import { App } from "./app";
import { takeToken } from "./token";
import "./tokens.css";
import "./base.css";
import "./components/components.css";
import "./shell.css";

// The token first, before anything else reads the address: it is taken out of the address
// bar at once (plan m5, "The token").
const take = () => takeToken(window.location, window.sessionStorage, (url) => window.history.replaceState(null, "", url));
const { token } = take();
const client = createClient({ token });

const root = document.getElementById("app");
if (root) {
  render(
    <App
      client={client}
      hasToken={token !== null}
      initialHash={window.location.hash}
      subscribeHash={(listener) => {
        const on = () => listener(window.location.hash);
        window.addEventListener("hashchange", on);
        return () => window.removeEventListener("hashchange", on);
      }}
      go={(hash) => {
        window.location.hash = hash;
      }}
      takeToken={take}
    />,
    root,
  );
}

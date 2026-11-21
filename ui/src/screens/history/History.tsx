import { EmptyScreen } from "../Empty";

/** History: every run, newest first, with its figures (plan m5, task W3). */
export function History() {
  return (
    <EmptyScreen title="History" link={{ href: "#/", words: "Back to Home" }}>
      No runs yet. Every run you start is recorded and listed here.
    </EmptyScreen>
  );
}

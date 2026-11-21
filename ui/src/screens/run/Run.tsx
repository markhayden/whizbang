import { EmptyScreen } from "../Empty";

export interface RunProps {
  id: string;
}

/** A run: running, or its result (plan m5, tasks W1 and W2). */
export function Run(props: RunProps) {
  return (
    <EmptyScreen title={`Run ${props.id}`} link={{ href: "#/history", words: "Open History" }}>
      This build cannot show a run in the playground yet.
    </EmptyScreen>
  );
}

import { EmptyScreen } from "../Empty";

export interface WorkspaceProps {
  kit: string;
  task?: string;
}

/** The kit workspace: a task's draft, inputs and plan (plan m5, task W1). */
export function Workspace(props: WorkspaceProps) {
  return (
    <EmptyScreen title={props.task ? `${props.kit} · ${props.task}` : props.kit} link={{ href: "#/", words: "Back to Home" }}>
      This task cannot be run from the playground in this build yet.
    </EmptyScreen>
  );
}

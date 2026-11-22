// A picture picked on Home or in ⌘K search (`#/?picture=<identity>`): the tasks that take a
// picture, and the draft that starts one with it. Pure, so each is tested alone
// (home.test.tsx); Home only lays them out.
import type { Hello, Picture, Task, TaskField } from "../../api";
import type { Draft } from "../../drafts";
import { fieldsOf, labelOf } from "../../form";

/** A task that can start from a picture, with the field the picture goes in. */
export interface Starter {
  kit: string;
  kitName: string;
  task: Task;
  field: TaskField;
  /** The field's name, as a person reads it. */
  label: string;
}

/** The field a picture goes in: the task's first that takes one picture, or a list of them. */
export function pictureFieldOf(task: Task): TaskField | undefined {
  const takes = (field: TaskField) => (field.kind === "picture" || field.kind === "pictures") && (!field.artifact || field.artifact === "image");
  return fieldsOf(task).find(takes);
}

/** Every kit's task that takes a picture, in the order hello lists them. */
export function startersOf(hello: Hello | null | undefined): Starter[] {
  const kits = Array.isArray(hello?.kits) ? hello.kits : [];
  return kits.flatMap((kit) =>
    (Array.isArray(kit.tasks) ? kit.tasks : []).flatMap((task) => {
      const field = pictureFieldOf(task);
      return field ? [{ kit: kit.id, kitName: kit.name ?? kit.id, task, field, label: labelOf(field) }] : [];
    }),
  );
}

/** The draft that starts a task with the picture in its field, and nothing else given. */
export function draftWith(starter: Starter, picture: Picture, at: number): Draft {
  const value = starter.field.kind === "pictures" ? [picture.id] : picture.id;
  return { kit: starter.kit, task: starter.task.name, values: { [starter.field.name]: value }, pictures: { [picture.id]: picture }, at };
}

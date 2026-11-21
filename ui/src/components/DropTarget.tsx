import { useState } from "preact/hooks";
import { cx } from "./cx";

export interface DropTargetProps {
  label: string;
  disabled?: boolean;
  /** Pictures dropped or chosen; only files whose type is an image are passed. */
  onFiles?: (files: File[]) => void;
  /** At most this many files at once. */
  max?: number;
}

/** The images among files, at most `max` of them. */
export function picturesOf(files: ArrayLike<File>, max = Infinity): File[] {
  return Array.from(files)
    .filter((file) => file.type.startsWith("image/"))
    .slice(0, max);
}

/** A dashed frame that takes dropped pictures, or opens the file chooser when clicked. */
export function DropTarget(props: DropTargetProps) {
  const [over, setOver] = useState(false);
  const take = (files: FileList | null | undefined) => {
    if (!files || props.disabled) return;
    const pictures = picturesOf(files, props.max);
    if (pictures.length > 0) props.onFiles?.(pictures);
  };
  return (
    <label
      class={cx("drop-target", over && "drop-target--over", props.disabled && "drop-target--disabled")}
      onDragOver={(event) => {
        event.preventDefault();
        if (!props.disabled) setOver(true);
      }}
      onDragLeave={() => setOver(false)}
      onDrop={(event) => {
        event.preventDefault();
        setOver(false);
        take(event.dataTransfer?.files);
      }}
    >
      <span class="drop-target__label">{props.label}</span>
      <input
        class="visually-hidden"
        type="file"
        accept="image/*"
        multiple={(props.max ?? 1) > 1}
        disabled={props.disabled}
        onChange={(event) => take((event.currentTarget as HTMLInputElement).files)}
      />
    </label>
  );
}

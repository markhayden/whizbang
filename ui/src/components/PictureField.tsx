import { DropTarget } from "./DropTarget";
import { Field } from "./Field";
import { PictureTile } from "./PictureTile";

export interface Picture {
  /** The picture's identity in the store. */
  id: string;
  /** Where the page shows it from: an address on the host, or a `blob:` address. */
  src: string;
  /** The file name it came with, shown as text. */
  name: string;
  width?: number;
  height?: number;
}

export interface PictureFieldProps {
  id: string;
  label: string;
  picture: Picture | null;
  optional?: boolean;
  help?: string;
  error?: string;
  disabled?: boolean;
  /** A tag beside the label, such as FROM RASCAL. */
  from?: string;
  onDrop?: (files: File[]) => void;
  onChoose?: () => void;
  onClear?: () => void;
}

/** One picture: from the library, from a drop, or cleared. */
export function PictureField(props: PictureFieldProps) {
  return (
    <Field id={props.id} label={props.label} optional={props.optional} help={props.help} error={props.error} disabled={props.disabled}>
      <div class="picture-field" id={props.id}>
        {props.from ? <span class="picture-field__from mono">{props.from}</span> : null}
        {props.picture ? (
          <div class="picture-field__chosen">
            <PictureTile src={props.picture.src} name={props.picture.name} width={props.picture.width} height={props.picture.height} />
            <div class="picture-field__meta">
              <span class="picture-field__name">{props.picture.name}</span>
              {props.picture.width && props.picture.height ? (
                <span class="picture-field__size mono">
                  {props.picture.width}×{props.picture.height}
                </span>
              ) : null}
              {props.disabled ? null : (
                <button type="button" class="link-button" onClick={props.onClear}>
                  Clear
                </button>
              )}
            </div>
          </div>
        ) : (
          <div class="picture-field__empty">
            <DropTarget label="Drop a picture" disabled={props.disabled} onFiles={props.onDrop} />
            {props.onChoose && !props.disabled ? (
              <button type="button" class="link-button" onClick={props.onChoose}>
                From the library
              </button>
            ) : null}
          </div>
        )}
      </div>
    </Field>
  );
}

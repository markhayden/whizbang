import { cx } from "./cx";

export interface PictureTileProps {
  src: string;
  /** The file name, used as the picture's text alternative. */
  name: string;
  width?: number;
  height?: number;
  selected?: boolean;
  onClick?: () => void;
}

/** A picture in the library or a field: default or selected. */
export function PictureTile(props: PictureTileProps) {
  const image = <img class="picture-tile__image" src={props.src} alt={props.name} width={props.width} height={props.height} loading="lazy" decoding="async" />;
  if (!props.onClick) return <span class={cx("picture-tile", props.selected && "picture-tile--selected")}>{image}</span>;
  return (
    <button type="button" class={cx("picture-tile", props.selected && "picture-tile--selected")} aria-pressed={props.selected ? "true" : "false"} title={props.name} onClick={props.onClick}>
      {image}
    </button>
  );
}

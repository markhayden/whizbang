// Pieces the screens share, made of U1's components: a stored picture shown by its
// identity, and the library of pictures a field picks from.
import type { Client, Picture } from "../api";
import { PictureTile } from "../components/PictureTile";
import { useArtifact } from "../hooks";

/** The edge of a thumbnail the page asks for. */
export const THUMB_EDGE = 256;

export interface StoredPictureProps {
  client: Client;
  picture: Picture;
  selected?: boolean;
  onClick?: () => void;
}

/** A picture of the store as a tile: its thumbnail, fetched with the token. */
export function StoredPicture(props: StoredPictureProps) {
  const src = useArtifact(props.client, props.picture.id, THUMB_EDGE);
  return <PictureTile src={src ?? ""} name={props.picture.name} width={props.picture.width} height={props.picture.height} selected={props.selected} onClick={props.onClick} />;
}

export interface LibraryProps {
  client: Client;
  pictures: Picture[];
  label: string;
  onPick: (picture: Picture) => void;
  /** Asks for the page after the last picture shown, when there may be one. */
  onMore?: () => void;
}

/** The pictures the playground imported, to pick one from. */
export function Library(props: LibraryProps) {
  if (props.pictures.length === 0) return <p class="empty">No pictures yet. Drop one to add it.</p>;
  return (
    <div class="library" role="group" aria-label={props.label}>
      {props.pictures.map((picture) => (
        <StoredPicture key={picture.id} client={props.client} picture={picture} onClick={() => props.onPick(picture)} />
      ))}
      {props.onMore ? (
        <button type="button" class="link-button" onClick={props.onMore}>
          More pictures
        </button>
      ) : null}
    </div>
  );
}

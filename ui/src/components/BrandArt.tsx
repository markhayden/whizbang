export interface BrandArtProps {
  /** The art (the emblem or the crew), a WebP under `ui/public/brand/`, once committed. */
  src?: string;
  /** Its text alternative. */
  alt: string;
  kind: "emblem" | "crew";
}

/** Where the owner's art goes (plan m5, task W3, item 6): the art, or an empty frame. */
export function BrandArt(props: BrandArtProps) {
  if (props.src) return <img class={`brand-art brand-art--${props.kind}`} src={props.src} alt={props.alt} />;
  return <span class={`brand-art brand-art--${props.kind} brand-art--empty`} role="presentation" data-brand-slot={props.kind} />;
}

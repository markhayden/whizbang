export interface WordmarkProps {
  /**
   * The outlined wordmark, an SVG under `ui/public/brand/`, when the owner's art is
   * committed (plan m5, task W3, item 6). Until then the word is set in type.
   */
  src?: string;
  size?: "bar" | "home";
}

/** "whizbang": the brand's hook. The art is the owner's and comes with W3. */
export function Wordmark(props: WordmarkProps) {
  const size = props.size ?? "bar";
  if (props.src) return <img class={`wordmark wordmark--${size}`} src={props.src} alt="whizbang" />;
  return <span class={`wordmark wordmark--${size} wordmark--type`}>whizbang</span>;
}

import { useEffect, useRef, useState } from "preact/hooks";
import { Segmented } from "./Segmented";

export type CompareMode = "side" | "swipe" | "difference";

export interface CompareProps {
  before: string;
  after: string;
  width: number;
  height: number;
  mode: CompareMode;
  /**
   * One scale for both pictures: 1 is their own size; "fit" scales them so the two fit
   * the stage side by side, whatever its size, as it is measured.
   */
  zoom: number | "fit";
  onMode?: (mode: CompareMode) => void;
  /** What the two pictures are called: Before and After unless said (Baseline and Attempt). */
  labels?: { before: string; after: string };
}

/**
 * The difference of two pictures of one size, as RGBA: each channel the absolute
 * difference, amplified so a small change shows, and opaque. Drawn in the page from the
 * two pictures it already has; no action makes it (plan m5, "What the design shows").
 */
export function differenceOf(a: Uint8ClampedArray, b: Uint8ClampedArray, gain = 4): Uint8ClampedArray {
  const out = new Uint8ClampedArray(Math.min(a.length, b.length));
  for (let i = 0; i + 3 < out.length; i += 4) {
    out[i] = Math.abs(a[i]! - b[i]!) * gain;
    out[i + 1] = Math.abs(a[i + 1]! - b[i + 1]!) * gain;
    out[i + 2] = Math.abs(a[i + 2]! - b[i + 2]!) * gain;
    out[i + 3] = 255;
  }
  return out;
}

function pixelsOf(src: string, width: number, height: number): Promise<Uint8ClampedArray | null> {
  return new Promise((resolve) => {
    const image = new Image();
    image.onload = () => {
      const canvas = document.createElement("canvas");
      canvas.width = width;
      canvas.height = height;
      const context = canvas.getContext("2d");
      if (!context) return resolve(null);
      context.drawImage(image, 0, 0, width, height);
      resolve(context.getImageData(0, 0, width, height).data);
    };
    image.onerror = () => resolve(null);
    image.src = src;
  });
}

/** The gap between the two pictures side by side, in pixels (`.compare__side`). */
const SIDE_GAP = 2;

/**
 * The one zoom at which two pictures of this size fit a stage of this size side by side:
 * at most 4, and never past the stage's width or, when it is known, its height.
 */
export function fitZoom(width: number, height: number, stage: { width: number; height: number }): number {
  if (width <= 0 || height <= 0 || stage.width <= 0) return 1;
  const across = (stage.width - SIDE_GAP) / (2 * width);
  const down = stage.height > 0 ? stage.height / height : Infinity;
  return Math.max(0.01, Math.min(4, across, down));
}

/** The size of an element, again whenever it changes; none until it is measured. */
function useMeasured(element: { current: HTMLElement | null }): { width: number; height: number } {
  const [size, setSize] = useState({ width: 0, height: 0 });
  useEffect(() => {
    const node = element.current;
    if (!node) return;
    const measure = () => setSize((now) => (now.width === node.clientWidth && now.height === node.clientHeight ? now : { width: node.clientWidth, height: node.clientHeight }));
    measure();
    if (typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(measure);
    observer.observe(node);
    return () => observer.disconnect();
  }, []);
  return size;
}

const MODES = [
  { value: "side", label: "Side by side" },
  { value: "swipe", label: "Swipe" },
  { value: "difference", label: "Difference" },
];

/** A source and its result: side by side, swiped, or their difference, at one zoom. */
export function Compare(props: CompareProps) {
  const [split, setSplit] = useState(50);
  const difference = useRef<HTMLCanvasElement>(null);
  const stage = useRef<HTMLDivElement>(null);
  const measured = useMeasured(stage);
  // A stage not measured yet (a test's page has no layout) fits a half of 960 pixels.
  const zoom = props.zoom === "fit" ? fitZoom(props.width, props.height, measured.width > 0 ? measured : { width: 960, height: 0 }) : props.zoom;
  useEffect(() => {
    if (props.mode !== "difference") return;
    let live = true;
    void Promise.all([pixelsOf(props.before, props.width, props.height), pixelsOf(props.after, props.width, props.height)]).then(([a, b]) => {
      const canvas = difference.current;
      const context = canvas?.getContext("2d");
      if (!live || !a || !b || !canvas || !context) return;
      const image = context.createImageData(props.width, props.height);
      image.data.set(differenceOf(a, b));
      context.putImageData(image, 0, 0);
    });
    return () => {
      live = false;
    };
  }, [props.mode, props.before, props.after, props.width, props.height]);
  const size = { width: `${Math.floor(props.width * zoom)}px`, height: `${Math.floor(props.height * zoom)}px` };
  const before = props.labels?.before ?? "Before";
  const after = props.labels?.after ?? "After";
  return (
    <div class="compare">
      <Segmented label="Compare" segments={MODES} value={props.mode} onChange={(mode) => props.onMode?.(mode as CompareMode)} />
      <div class="compare__stage" ref={stage}>
        {props.mode === "side" ? (
          <div class="compare__side">
            <figure class="compare__figure" style={size}>
              <img src={props.before} alt={before} />
              <figcaption class="compare__badge mono">{before.toUpperCase()}</figcaption>
            </figure>
            <figure class="compare__figure" style={size}>
              <img src={props.after} alt={after} />
              <figcaption class="compare__badge mono">{after.toUpperCase()}</figcaption>
            </figure>
          </div>
        ) : null}
        {props.mode === "swipe" ? (
          <figure class="compare__figure compare__swipe" style={size}>
            <img src={props.after} alt={after} />
            <img class="compare__top" src={props.before} alt={before} style={{ clipPath: `inset(0 ${100 - split}% 0 0)` }} />
            <span class="compare__line" style={{ left: `${split}%` }} aria-hidden="true" />
            <input class="compare__range" type="range" min={0} max={100} value={split} aria-label={`Swipe between ${before.toLowerCase()} and ${after.toLowerCase()}`} onInput={(event) => setSplit(Number((event.currentTarget as HTMLInputElement).value))} />
            <span class="compare__badge mono">{before.toUpperCase()}</span>
            <span class="compare__badge compare__badge--end mono">{after.toUpperCase()}</span>
          </figure>
        ) : null}
        {props.mode === "difference" ? (
          <figure class="compare__figure" style={size}>
            <canvas ref={difference} class="compare__difference" width={props.width} height={props.height} aria-label={`Difference between ${before.toLowerCase()} and ${after.toLowerCase()}`} />
            <figcaption class="compare__badge mono">DIFFERENCE</figcaption>
          </figure>
        ) : null}
      </div>
    </div>
  );
}

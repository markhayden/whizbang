import { useRef, useState } from "preact/hooks";
import { cx } from "./cx";

/** A point in the picture's own pixels. */
export type Point = [number, number];

/** What the canvas draws over a picture, in the picture's pixels. */
export type Mark =
  | { kind: "box"; x: number; y: number; w: number; h: number; label?: string }
  | { kind: "mask"; src: string; label?: string }
  | { kind: "point"; x: number; y: number }
  | { kind: "stroke"; points: Point[]; width: number }
  | { kind: "flag"; x: number; y: number; w: number; h: number; words: string };

/** What a person can draw: a box (a drag), a point (a click) or a stroke. */
export type DrawTool = "box" | "point" | "stroke";

export interface CanvasProps {
  src: string;
  /** The picture's name, its text alternative. */
  name: string;
  width: number;
  height: number;
  marks: Mark[];
  /** A scale (1 is the picture's own size), or fit to the frame. */
  zoom: number | "fit";
  tool?: DrawTool | null;
  onDraw?: (mark: Mark) => void;
  onZoom?: (zoom: number | "fit") => void;
}

/** A pointer's place on the drawn picture, in the picture's pixels, held to the picture. */
export function toPicture(clientX: number, clientY: number, rect: { left: number; top: number; width: number; height: number }, width: number, height: number): Point {
  if (rect.width <= 0 || rect.height <= 0) return [0, 0];
  const x = ((clientX - rect.left) / rect.width) * width;
  const y = ((clientY - rect.top) / rect.height) * height;
  return [Math.round(Math.min(width, Math.max(0, x))), Math.round(Math.min(height, Math.max(0, y)))];
}

/** The box two corners span, whichever way the drag went. */
export function boxOf(a: Point, b: Point): { x: number; y: number; w: number; h: number } {
  return { x: Math.min(a[0], b[0]), y: Math.min(a[1], b[1]), w: Math.abs(a[0] - b[0]), h: Math.abs(a[1] - b[1]) };
}

const ZOOMS = [0.25, 0.5, 1, 2, 4];

/** The next zoom in or out from a scale. */
export function stepZoom(zoom: number, direction: 1 | -1): number {
  if (direction > 0) return ZOOMS.find((z) => z > zoom + 1e-9) ?? ZOOMS[ZOOMS.length - 1]!;
  return [...ZOOMS].reverse().find((z) => z < zoom - 1e-9) ?? ZOOMS[0]!;
}

/**
 * A picture with its marks (a box, a mask laid over it, a point, a stroke, a judge's flag
 * with its words), zoomed or fit to its frame; with a tool, it takes a drawn mark.
 */
export function Canvas(props: CanvasProps) {
  const svg = useRef<SVGSVGElement>(null);
  const [drag, setDrag] = useState<{ from: Point; to: Point; points: Point[] } | null>(null);
  const at = (event: PointerEvent): Point => {
    const rect = svg.current?.getBoundingClientRect() ?? { left: 0, top: 0, width: 0, height: 0 };
    return toPicture(event.clientX, event.clientY, rect, props.width, props.height);
  };
  const end = (event: PointerEvent) => {
    if (!drag || !props.tool) return;
    const to = at(event);
    if (props.tool === "point") props.onDraw?.({ kind: "point", x: to[0], y: to[1] });
    if (props.tool === "box") {
      const box = boxOf(drag.from, to);
      if (box.w > 0 && box.h > 0) props.onDraw?.({ kind: "box", ...box });
    }
    if (props.tool === "stroke" && drag.points.length > 1) props.onDraw?.({ kind: "stroke", points: [...drag.points, to], width: 12 });
    setDrag(null);
  };
  const inner = props.zoom === "fit" ? { aspectRatio: `${props.width} / ${props.height}` } : { width: `${props.width * props.zoom}px`, height: `${props.height * props.zoom}px` };
  const live = drag && props.tool === "box" ? boxOf(drag.from, drag.to) : null;
  const fit = props.zoom === "fit";
  return (
    <div class="canvas">
      <div class={cx("canvas__frame", fit && "canvas__frame--fit")}>
        <div class="canvas__inner" style={inner}>
          <img class="canvas__picture" src={props.src} alt={props.name} draggable={false} />
          <svg
            ref={svg}
            class={cx("canvas__marks", props.tool && "canvas__marks--drawing")}
            viewBox={`0 0 ${props.width} ${props.height}`}
            preserveAspectRatio="none"
            onPointerDown={(event) => {
              if (!props.tool) return;
              const from = at(event);
              setDrag({ from, to: from, points: [from] });
            }}
            onPointerMove={(event) => {
              if (!drag) return;
              const to = at(event);
              setDrag({ from: drag.from, to, points: props.tool === "stroke" ? [...drag.points, to] : drag.points });
            }}
            onPointerUp={end}
          >
            <defs>
              {props.marks.map((mark, index) =>
                mark.kind === "mask" ? (
                  <mask id={`canvas-mask-${index}`} key={index}>
                    <image href={mark.src} x="0" y="0" width={props.width} height={props.height} preserveAspectRatio="none" />
                  </mask>
                ) : null,
              )}
            </defs>
            {props.marks.map((mark, index) => {
              switch (mark.kind) {
                case "mask":
                  return <rect key={index} class="canvas__mask" x="0" y="0" width={props.width} height={props.height} mask={`url(#canvas-mask-${index})`} />;
                case "box":
                  return <rect key={index} class="canvas__box" x={mark.x} y={mark.y} width={mark.w} height={mark.h} />;
                case "flag":
                  return <rect key={index} class="canvas__flag" x={mark.x} y={mark.y} width={mark.w} height={mark.h} />;
                case "point":
                  return <circle key={index} class="canvas__point" cx={mark.x} cy={mark.y} r={Math.max(props.width, props.height) / 100} />;
                case "stroke":
                  return <polyline key={index} class="canvas__stroke" points={mark.points.map((p) => p.join(",")).join(" ")} style={{ strokeWidth: `${mark.width}px` }} />;
              }
            })}
            {live ? <rect class="canvas__box canvas__box--live" x={live.x} y={live.y} width={live.w} height={live.h} /> : null}
          </svg>
          {props.marks.map((mark, index) => {
            const words = mark.kind === "flag" ? mark.words : mark.kind === "box" || mark.kind === "mask" ? mark.label : undefined;
            if (!words) return null;
            const left = mark.kind === "flag" || mark.kind === "box" ? (mark.x / props.width) * 100 : 2;
            const top = mark.kind === "flag" || mark.kind === "box" ? (mark.y / props.height) * 100 : 90;
            return (
              <span key={`label-${index}`} class={`canvas__label canvas__label--${mark.kind} mono`} style={{ left: `${left}%`, top: `${top}%` }}>
                {words}
              </span>
            );
          })}
        </div>
      </div>
      <div class="canvas__zoom mono" role="group" aria-label="Zoom">
        <button type="button" class="icon-button" aria-label="Zoom out" onClick={() => props.onZoom?.(stepZoom(props.zoom === "fit" ? 1 : props.zoom, -1))}>
          −
        </button>
        <button type="button" class={cx("canvas__fit", fit && "canvas__fit--on")} aria-pressed={fit ? "true" : "false"} onClick={() => props.onZoom?.("fit")}>
          {fit ? "fit" : `${Math.round((props.zoom as number) * 100)}%`}
        </button>
        <button type="button" class="icon-button" aria-label="Zoom in" onClick={() => props.onZoom?.(stepZoom(props.zoom === "fit" ? 1 : props.zoom, 1))}>
          +
        </button>
      </div>
    </div>
  );
}

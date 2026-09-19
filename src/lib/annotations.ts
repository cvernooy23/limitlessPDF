/**
 * Annotation model. Coordinates are stored in the page's *unscaled* viewport
 * space (scale = 1, top-left origin), so they survive zoom — render multiplies
 * by the current scale. Conversion to true PDF coordinates happens when we burn
 * annotations into the file (Rust/PDFium step).
 */

export type Tool = "none" | "highlight" | "draw" | "note" | "text" | "edittext";

export interface Point {
  x: number;
  y: number;
}
export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

interface Base {
  id: string;
  /** Stable page-instance key (matches PageItem.key), unique across documents. */
  pageKey: string;
  color: string;
}
export interface HighlightAnnotation extends Base {
  type: "highlight";
  rect: Rect;
}
export interface DrawAnnotation extends Base {
  type: "draw";
  paths: Point[][];
  width: number;
}
export interface NoteAnnotation extends Base {
  type: "note";
  x: number;
  y: number;
  text: string;
}
export type Annotation = HighlightAnnotation | DrawAnnotation | NoteAnnotation;

export const ANNOTATION_COLORS = ["#ffd23f", "#ff7a90", "#6ee7b7", "#6ea8ff", "#c4a3ff"];

export function newId(): string {
  return `a_${Math.random().toString(36).slice(2, 10)}`;
}

/** Distance from point (px,py) to segment (ax,ay)-(bx,by). */
function segmentDistance(
  px: number,
  py: number,
  ax: number,
  ay: number,
  bx: number,
  by: number,
): number {
  const dx = bx - ax;
  const dy = by - ay;
  const len2 = dx * dx + dy * dy;
  let t = len2 === 0 ? 0 : ((px - ax) * dx + (py - ay) * dy) / len2;
  t = Math.max(0, Math.min(1, t));
  const cx = ax + t * dx;
  const cy = ay + t * dy;
  return Math.hypot(px - cx, py - cy);
}

/**
 * Geometric hit-test in page (CSS-pixel) space. Coordinates are stored at
 * scale 1, so multiply by the current scale. Returns the id of the topmost
 * annotation under (x, y), or null. Done in JS rather than via SVG hit-testing,
 * which proved unreliable inside the webview.
 */
export function hitTest(annos: Annotation[], x: number, y: number, scale: number): string | null {
  for (let i = annos.length - 1; i >= 0; i--) {
    const a = annos[i];
    if (a.type === "highlight") {
      const x0 = a.rect.x * scale;
      const y0 = a.rect.y * scale;
      const x1 = (a.rect.x + a.rect.w) * scale;
      const y1 = (a.rect.y + a.rect.h) * scale;
      if (x >= x0 && x <= x1 && y >= y0 && y <= y1) return a.id;
    } else if (a.type === "note") {
      const cx = a.x * scale;
      const cy = a.y * scale;
      // Pin body sits in x:[cx-10,cx+10], y:[cy-24,cy] (see AnnotationLayer).
      if (x >= cx - 11 && x <= cx + 11 && y >= cy - 26 && y <= cy + 3) return a.id;
    } else if (a.type === "draw") {
      const tol = Math.max(a.width * scale, 8);
      for (const stroke of a.paths) {
        if (stroke.length === 1) {
          if (Math.hypot(x - stroke[0].x * scale, y - stroke[0].y * scale) <= tol) return a.id;
          continue;
        }
        for (let j = 0; j < stroke.length - 1; j++) {
          const d = segmentDistance(
            x,
            y,
            stroke[j].x * scale,
            stroke[j].y * scale,
            stroke[j + 1].x * scale,
            stroke[j + 1].y * scale,
          );
          if (d <= tol) return a.id;
        }
      }
    }
  }
  return null;
}

/** Build an SVG path string from ink strokes, scaled to the current zoom. */
export function inkToPath(paths: Point[][], scale: number): string {
  return paths
    .map((stroke) =>
      stroke
        .map(
          (p, i) => `${i === 0 ? "M" : "L"}${(p.x * scale).toFixed(2)} ${(p.y * scale).toFixed(2)}`,
        )
        .join(" "),
    )
    .join(" ");
}

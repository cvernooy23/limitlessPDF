<script lang="ts">
  import {
    newId,
    inkToPath,
    type Annotation,
    type Point,
    type Rect,
    type Tool,
  } from "./annotations";

  const {
    pageKey,
    scale,
    tool,
    color,
    inkWidth = 2.5,
    annotations,
    selectedId = null,
    onAdd,
    onSelect,
  }: {
    pageKey: string;
    scale: number;
    tool: Tool;
    color: string;
    inkWidth?: number;
    annotations: Annotation[];
    selectedId?: string | null;
    onAdd: (a: Annotation) => void;
    onSelect: (id: string | null) => void;
  } = $props();

  let svgEl: SVGSVGElement;
  let drawing = $state(false);
  let start: Point | null = null;
  let draftRect = $state<{ x: number; y: number; w: number; h: number } | null>(null);
  let draftInk = $state<Point[] | null>(null);
  let draftLine = $state<{ rect: Rect; mode: "underline" | "strikethrough" } | null>(null);
  let draftShape = $state<{
    x: number;
    y: number;
    w: number;
    h: number;
    kind: "rect" | "circle";
  } | null>(null);
  let draftArrow = $state<{ start: Point; end: Point } | null>(null);
  let draftRedact = $state<{ x: number; y: number; w: number; h: number } | null>(null);

  function toLocal(e: PointerEvent): Point {
    const r = svgEl.getBoundingClientRect();
    return { x: (e.clientX - r.left) / scale, y: (e.clientY - r.top) / scale };
  }

  /** Compute arrowhead polygon points for an arrow from s to e. */
  function arrowHead(s: Point, e: Point, sc: number): string {
    const dx = e.x - s.x;
    const dy = e.y - s.y;
    const len = Math.hypot(dx, dy);
    if (len === 0) return "";
    const ux = dx / len;
    const uy = dy / len;
    const headLen = Math.min(12 / sc, len * 0.4);
    const headW = headLen * 0.5;
    const bx = e.x - ux * headLen;
    const by = e.y - uy * headLen;
    const p1x = (bx - uy * headW) * sc;
    const p1y = (by + ux * headW) * sc;
    const p2x = (bx + uy * headW) * sc;
    const p2y = (by - ux * headW) * sc;
    const tx = e.x * sc;
    const ty = e.y * sc;
    return `${p1x},${p1y} ${tx},${ty} ${p2x},${p2y}`;
  }

  function onDown(e: PointerEvent) {
    // Select-mode clicks are handled geometrically on the HTML page container
    // (see PdfViewer.selectAt); this SVG only handles drawing.
    if (tool === "none") return;
    e.preventDefault();
    (e.target as Element).setPointerCapture?.(e.pointerId);
    const p = toLocal(e);

    if (tool === "note") {
      const a: Annotation = { id: newId(), pageKey, color, type: "note", x: p.x, y: p.y, text: "" };
      onAdd(a);
      onSelect(a.id);
      return;
    }

    drawing = true;
    start = p;
    if (tool === "highlight") draftRect = { x: p.x, y: p.y, w: 0, h: 0 };
    else if (tool === "underline" || tool === "strikethrough")
      draftLine = { rect: { x: p.x, y: p.y, w: 0, h: 0 }, mode: tool };
    else if (tool === "draw") draftInk = [p];
    else if (tool === "rect") draftShape = { x: p.x, y: p.y, w: 0, h: 0, kind: "rect" };
    else if (tool === "circle") draftShape = { x: p.x, y: p.y, w: 0, h: 0, kind: "circle" };
    else if (tool === "arrow") draftArrow = { start: { ...p }, end: { ...p } };
    else if (tool === "redact") draftRedact = { x: p.x, y: p.y, w: 0, h: 0 };
  }

  function onMove(e: PointerEvent) {
    if (!drawing) return;
    const p = toLocal(e);
    if (tool === "highlight" && start) {
      draftRect = {
        x: Math.min(start.x, p.x),
        y: Math.min(start.y, p.y),
        w: Math.abs(p.x - start.x),
        h: Math.abs(p.y - start.y),
      };
    } else if ((tool === "underline" || tool === "strikethrough") && draftLine && start) {
      draftLine = {
        ...draftLine,
        rect: {
          x: Math.min(start.x, p.x),
          y: Math.min(start.y, p.y),
          w: Math.abs(p.x - start.x),
          h: Math.abs(p.y - start.y),
        },
      };
    } else if (tool === "draw" && draftInk) {
      draftInk = [...draftInk, p];
    } else if ((tool === "rect" || tool === "circle") && draftShape && start) {
      draftShape = {
        ...draftShape,
        x: Math.min(start.x, p.x),
        y: Math.min(start.y, p.y),
        w: Math.abs(p.x - start.x),
        h: Math.abs(p.y - start.y),
      };
    } else if (tool === "arrow" && draftArrow) {
      draftArrow = { ...draftArrow, end: { ...p } };
    }
  }

  function onUp() {
    if (!drawing) return;
    drawing = false;
    if (tool === "highlight" && draftRect && draftRect.w > 3 && draftRect.h > 3) {
      onAdd({ id: newId(), pageKey, color, type: "highlight", rect: { ...draftRect } });
    } else if (
      (tool === "underline" || tool === "strikethrough") &&
      draftLine &&
      draftLine.rect.w > 3 &&
      draftLine.rect.h > 3
    ) {
      onAdd({ id: newId(), pageKey, color, type: draftLine.mode, rect: { ...draftLine.rect } });
    } else if (tool === "draw" && draftInk && draftInk.length > 1) {
      onAdd({ id: newId(), pageKey, color, type: "draw", paths: [draftInk], width: inkWidth });
    } else if (tool === "rect" && draftShape && draftShape.w > 3 && draftShape.h > 3) {
      onAdd({
        id: newId(),
        pageKey,
        color,
        type: "rect",
        rect: { x: draftShape.x, y: draftShape.y, w: draftShape.w, h: draftShape.h },
        borderWidth: inkWidth,
      });
    } else if (tool === "circle" && draftShape && draftShape.w > 3 && draftShape.h > 3) {
      onAdd({
        id: newId(),
        pageKey,
        color,
        type: "circle",
        rect: { x: draftShape.x, y: draftShape.y, w: draftShape.w, h: draftShape.h },
        borderWidth: inkWidth,
      });
    } else if (tool === "arrow" && draftArrow) {
      const dx = draftArrow.end.x - draftArrow.start.x;
      const dy = draftArrow.end.y - draftArrow.start.y;
      if (Math.hypot(dx, dy) > 5) {
        onAdd({
          id: newId(),
          pageKey,
          color,
          type: "arrow",
          start: { ...draftArrow.start },
          end: { ...draftArrow.end },
          width: inkWidth,
        });
      }
    }
    draftRect = null;
    draftInk = null;
    draftLine = null;
    draftShape = null;
    draftArrow = null;
    start = null;
  }

  // Drawing tools that this SVG handles. "text" is handled by TextBoxLayer.
  const isActive = $derived(
    tool === "highlight" ||
      tool === "underline" ||
      tool === "strikethrough" ||
      tool === "draw" ||
      tool === "note" ||
      tool === "rect" ||
      tool === "circle" ||
      tool === "arrow" ||
      tool === "redact",
  );
</script>

<!--
  The SVG root is left at the default `pointer-events: visiblePainted`, so empty
  areas pass clicks through to the text layer (selection) while painted shapes
  catch clicks (annotation selection). A transparent capture rect is added only
  while a drawing tool is active, so strokes/notes register over empty space.
-->
<svg
  bind:this={svgEl}
  class="anno-layer"
  style={`pointer-events: ${isActive ? "all" : "none"}; cursor: ${isActive ? "crosshair" : "default"};`}
  onpointerdown={onDown}
  onpointermove={onMove}
  onpointerup={onUp}
  onpointerleave={onUp}
>
  {#if isActive}
    <rect x="0" y="0" width="100%" height="100%" fill="transparent" style="pointer-events: all;" />
  {/if}

  {#each annotations as a (a.id)}
    {#if a.type === "highlight"}
      <rect
        x={a.rect.x * scale}
        y={a.rect.y * scale}
        width={a.rect.w * scale}
        height={a.rect.h * scale}
        rx="2"
        fill={a.color}
        fill-opacity="0.35"
        style="mix-blend-mode: multiply; pointer-events: none;"
        class="shape"
        class:selected={a.id === selectedId}
      />
    {:else if a.type === "underline"}
      <line
        x1={a.rect.x * scale}
        y1={(a.rect.y + a.rect.h) * scale}
        x2={(a.rect.x + a.rect.w) * scale}
        y2={(a.rect.y + a.rect.h) * scale}
        stroke={a.color}
        stroke-width={2 * scale}
        stroke-linecap="round"
        style="pointer-events: none;"
        class="shape line-shape"
        class:selected={a.id === selectedId}
      />
    {:else if a.type === "strikethrough"}
      <line
        x1={a.rect.x * scale}
        y1={(a.rect.y + a.rect.h / 2) * scale}
        x2={(a.rect.x + a.rect.w) * scale}
        y2={(a.rect.y + a.rect.h / 2) * scale}
        stroke={a.color}
        stroke-width={2 * scale}
        stroke-linecap="round"
        style="pointer-events: none;"
        class="shape line-shape"
        class:selected={a.id === selectedId}
      />
    {:else if a.type === "draw"}
      <path
        d={inkToPath(a.paths, scale)}
        stroke={a.color}
        stroke-width={a.width * scale}
        fill="none"
        stroke-linecap="round"
        stroke-linejoin="round"
        style="pointer-events: none;"
        class="shape"
        class:selected={a.id === selectedId}
      />
    {:else if a.type === "note"}
      <g
        class="note"
        class:selected={a.id === selectedId}
        style="pointer-events: none;"
        transform={`translate(${a.x * scale - 10}, ${a.y * scale - 24})`}
      >
        <path
          d="M10 0c5.5 0 10 4 10 9 0 6.5-10 15-10 15S0 15.5 0 9C0 4 4.5 0 10 0z"
          fill={a.color}
          stroke="rgba(0,0,0,.35)"
        />
        <circle cx="10" cy="9" r="3.2" fill="rgba(0,0,0,.55)" />
      </g>
    {:else if a.type === "rect"}
      <rect
        x={a.rect.x * scale}
        y={a.rect.y * scale}
        width={a.rect.w * scale}
        height={a.rect.h * scale}
        rx="2"
        fill="none"
        stroke={a.color}
        stroke-width={a.borderWidth * scale}
        style="pointer-events: none;"
        class="shape"
        class:selected={a.id === selectedId}
      />
    {:else if a.type === "circle"}
      <ellipse
        cx={(a.rect.x + a.rect.w / 2) * scale}
        cy={(a.rect.y + a.rect.h / 2) * scale}
        rx={(a.rect.w / 2) * scale}
        ry={(a.rect.h / 2) * scale}
        fill="none"
        stroke={a.color}
        stroke-width={a.borderWidth * scale}
        style="pointer-events: none;"
        class="shape"
        class:selected={a.id === selectedId}
      />
    {:else if a.type === "arrow"}
      <line
        x1={a.start.x * scale}
        y1={a.start.y * scale}
        x2={a.end.x * scale}
        y2={a.end.y * scale}
        stroke={a.color}
        stroke-width={a.width * scale}
        stroke-linecap="round"
        style="pointer-events: none;"
        class="shape line-shape"
        class:selected={a.id === selectedId}
      />
      <polygon
        points={arrowHead(a.start, a.end, scale)}
        fill={a.color}
        style="pointer-events: none;"
        class="shape"
        class:selected={a.id === selectedId}
      />
    {/if}
  {/each}

  {#if draftRect}
    <rect
      x={draftRect.x * scale}
      y={draftRect.y * scale}
      width={draftRect.w * scale}
      height={draftRect.h * scale}
      rx="2"
      fill={color}
      fill-opacity="0.3"
      style="mix-blend-mode: multiply"
    />
  {/if}
  {#if draftLine}
    <line
      x1={draftLine.rect.x * scale}
      y1={(draftLine.mode === "underline"
        ? draftLine.rect.y + draftLine.rect.h
        : draftLine.rect.y + draftLine.rect.h / 2) * scale}
      x2={(draftLine.rect.x + draftLine.rect.w) * scale}
      y2={(draftLine.mode === "underline"
        ? draftLine.rect.y + draftLine.rect.h
        : draftLine.rect.y + draftLine.rect.h / 2) * scale}
      stroke={color}
      stroke-width={2 * scale}
      stroke-linecap="round"
    />
  {/if}
  {#if draftInk}
    <path
      d={inkToPath([draftInk], scale)}
      stroke={color}
      stroke-width={inkWidth * scale}
      fill="none"
      stroke-linecap="round"
      stroke-linejoin="round"
    />
  {/if}
  {#if draftShape && draftShape.kind === "rect"}
    <rect
      x={draftShape.x * scale}
      y={draftShape.y * scale}
      width={draftShape.w * scale}
      height={draftShape.h * scale}
      rx="2"
      fill="none"
      stroke={color}
      stroke-width={inkWidth * scale}
      stroke-dasharray="4"
    />
  {/if}
  {#if draftShape && draftShape.kind === "circle"}
    <ellipse
      cx={(draftShape.x + draftShape.w / 2) * scale}
      cy={(draftShape.y + draftShape.h / 2) * scale}
      rx={(draftShape.w / 2) * scale}
      ry={(draftShape.h / 2) * scale}
      fill="none"
      stroke={color}
      stroke-width={inkWidth * scale}
      stroke-dasharray="4"
    />
  {/if}
  {#if draftArrow}
    <line
      x1={draftArrow.start.x * scale}
      y1={draftArrow.start.y * scale}
      x2={draftArrow.end.x * scale}
      y2={draftArrow.end.y * scale}
      stroke={color}
      stroke-width={inkWidth * scale}
      stroke-linecap="round"
      stroke-dasharray="4"
    />
    <polygon
      points={arrowHead(draftArrow.start, draftArrow.end, scale)}
      fill={color}
      opacity="0.7"
    />
  {/if}
  {#if draftRedact}
    <rect
      x={draftRedact.x * scale}
      y={draftRedact.y * scale}
      width={draftRedact.w * scale}
      height={draftRedact.h * scale}
      fill="#000000"
      fill-opacity="0.5"
      stroke="#ff0000"
      stroke-width="1"
      stroke-dasharray="4"
    />
  {/if}
</svg>

<style>
  .anno-layer {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    /* pointer-events + cursor are set inline based on the active tool. */
    z-index: 4;
    overflow: visible;
  }
  rect.shape.selected {
    stroke: var(--color-accent);
    stroke-width: 1.5;
  }
  line.shape.selected {
    filter: drop-shadow(0 0 3px var(--color-accent));
  }
  path.shape.selected {
    filter: drop-shadow(0 0 3px var(--color-accent));
  }
  .note.selected {
    filter: drop-shadow(0 0 4px var(--color-accent));
  }
  ellipse.shape.selected {
    stroke: var(--color-accent);
    stroke-width: 1.5;
  }
  polygon.shape.selected {
    filter: drop-shadow(0 0 3px var(--color-accent));
  }
</style>

<script lang="ts">
  import { newId, inkToPath, type Annotation, type Point, type Tool } from "./annotations";

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

  function toLocal(e: PointerEvent): Point {
    const r = svgEl.getBoundingClientRect();
    return { x: (e.clientX - r.left) / scale, y: (e.clientY - r.top) / scale };
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
    else if (tool === "draw") draftInk = [p];
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
    } else if (tool === "draw" && draftInk) {
      draftInk = [...draftInk, p];
    }
  }

  function onUp() {
    if (!drawing) return;
    drawing = false;
    if (tool === "highlight" && draftRect && draftRect.w > 3 && draftRect.h > 3) {
      onAdd({ id: newId(), pageKey, color, type: "highlight", rect: { ...draftRect } });
    } else if (tool === "draw" && draftInk && draftInk.length > 1) {
      onAdd({ id: newId(), pageKey, color, type: "draw", paths: [draftInk], width: inkWidth });
    }
    draftRect = null;
    draftInk = null;
    start = null;
  }

  // Drawing tools that this SVG handles. "text" is handled by TextBoxLayer.
  const isActive = $derived(tool === "highlight" || tool === "draw" || tool === "note");
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
  path.shape.selected {
    filter: drop-shadow(0 0 3px var(--color-accent));
  }
  .note.selected {
    filter: drop-shadow(0 0 4px var(--color-accent));
  }
</style>

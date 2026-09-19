import { describe, it, expect } from "vitest";
import {
  newId,
  hitTest,
  inkToPath,
  ANNOTATION_COLORS,
  type HighlightAnnotation,
  type UnderlineAnnotation,
  type StrikethroughAnnotation,
  type DrawAnnotation,
  type NoteAnnotation,
} from "./annotations";

// ---------------------------------------------------------------------------
// newId
// ---------------------------------------------------------------------------

describe("newId", () => {
  it("returns a string starting with 'a_'", () => {
    const id = newId();
    expect(id).toMatch(/^a_[a-z0-9]+$/);
  });

  it("generates unique ids", () => {
    const ids = new Set(Array.from({ length: 100 }, () => newId()));
    expect(ids.size).toBe(100);
  });
});

// ---------------------------------------------------------------------------
// ANNOTATION_COLORS
// ---------------------------------------------------------------------------

describe("ANNOTATION_COLORS", () => {
  it("contains 5 hex color strings", () => {
    expect(ANNOTATION_COLORS).toHaveLength(5);
    for (const c of ANNOTATION_COLORS) {
      expect(c).toMatch(/^#[0-9a-f]{6}$/);
    }
  });
});

// ---------------------------------------------------------------------------
// hitTest
// ---------------------------------------------------------------------------

describe("hitTest", () => {
  const base = { pageKey: "p1", color: "#ffd23f" };

  it("returns null for empty array", () => {
    expect(hitTest([], 50, 50, 1)).toBeNull();
  });

  // Highlight
  describe("highlight", () => {
    const hl: HighlightAnnotation = {
      ...base,
      id: "h1",
      type: "highlight",
      rect: { x: 10, y: 10, w: 100, h: 20 },
    };

    it("hits inside rect", () => {
      expect(hitTest([hl], 50, 20, 1)).toBe("h1");
    });

    it("misses outside rect", () => {
      expect(hitTest([hl], 5, 5, 1)).toBeNull();
    });

    it("respects scale", () => {
      // At scale 2, rect spans [20,20]-[220,60]. Point (30,30) is inside.
      expect(hitTest([hl], 30, 30, 2)).toBe("h1");
      // Point (15,15) is outside at scale 2.
      expect(hitTest([hl], 15, 15, 2)).toBeNull();
    });
  });

  // Underline
  describe("underline", () => {
    const ul: UnderlineAnnotation = {
      ...base,
      id: "u1",
      type: "underline",
      rect: { x: 10, y: 10, w: 100, h: 20 },
    };

    it("hits inside rect", () => {
      expect(hitTest([ul], 50, 20, 1)).toBe("u1");
    });

    it("misses outside rect", () => {
      expect(hitTest([ul], 5, 5, 1)).toBeNull();
    });
  });

  // Strikethrough
  describe("strikethrough", () => {
    const st: StrikethroughAnnotation = {
      ...base,
      id: "s1",
      type: "strikethrough",
      rect: { x: 10, y: 10, w: 100, h: 20 },
    };

    it("hits inside rect", () => {
      expect(hitTest([st], 60, 15, 1)).toBe("s1");
    });

    it("misses outside rect", () => {
      expect(hitTest([st], 200, 200, 1)).toBeNull();
    });
  });

  // Note
  describe("note", () => {
    const note: NoteAnnotation = {
      ...base,
      id: "n1",
      type: "note",
      x: 50,
      y: 50,
      text: "Hello",
    };

    it("hits the pin area", () => {
      // Pin body is roughly x:[39,61], y:[24,53] at scale 1
      expect(hitTest([note], 50, 40, 1)).toBe("n1");
    });

    it("misses far away", () => {
      expect(hitTest([note], 100, 100, 1)).toBeNull();
    });
  });

  // Draw
  describe("draw", () => {
    const draw: DrawAnnotation = {
      ...base,
      id: "d1",
      type: "draw",
      width: 2,
      paths: [
        [
          { x: 10, y: 10 },
          { x: 100, y: 10 },
        ],
      ],
    };

    it("hits near a stroke segment", () => {
      expect(hitTest([draw], 50, 10, 1)).toBe("d1");
    });

    it("misses far from strokes", () => {
      expect(hitTest([draw], 50, 100, 1)).toBeNull();
    });

    it("hits a single-point stroke", () => {
      const dot: DrawAnnotation = {
        ...base,
        id: "d2",
        type: "draw",
        width: 2,
        paths: [[{ x: 50, y: 50 }]],
      };
      expect(hitTest([dot], 50, 50, 1)).toBe("d2");
    });
  });

  // Ordering: last annotation wins (topmost)
  it("returns the topmost (last) annotation", () => {
    const a1: HighlightAnnotation = {
      ...base,
      id: "first",
      type: "highlight",
      rect: { x: 0, y: 0, w: 100, h: 100 },
    };
    const a2: HighlightAnnotation = {
      ...base,
      id: "second",
      type: "highlight",
      rect: { x: 0, y: 0, w: 100, h: 100 },
    };
    expect(hitTest([a1, a2], 50, 50, 1)).toBe("second");
  });
});

// ---------------------------------------------------------------------------
// inkToPath
// ---------------------------------------------------------------------------

describe("inkToPath", () => {
  it("builds an SVG path from strokes at scale 1", () => {
    const paths = [
      [
        { x: 0, y: 0 },
        { x: 10, y: 20 },
      ],
    ];
    const result = inkToPath(paths, 1);
    expect(result).toBe("M0.00 0.00 L10.00 20.00");
  });

  it("applies scale", () => {
    const paths = [[{ x: 5, y: 10 }]];
    const result = inkToPath(paths, 2);
    expect(result).toBe("M10.00 20.00");
  });

  it("joins multiple strokes with spaces", () => {
    const paths = [
      [
        { x: 0, y: 0 },
        { x: 1, y: 1 },
      ],
      [
        { x: 5, y: 5 },
        { x: 6, y: 6 },
      ],
    ];
    const result = inkToPath(paths, 1);
    expect(result).toContain("M0.00 0.00 L1.00 1.00");
    expect(result).toContain("M5.00 5.00 L6.00 6.00");
  });

  it("returns empty string for empty paths", () => {
    expect(inkToPath([], 1)).toBe("");
  });
});

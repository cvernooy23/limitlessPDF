import { describe, it, expect } from "vitest";
import { UndoManager } from "./history";

/** Simple snapshot type for testing. */
interface TestState {
  items: string[];
  count: number;
}

function state(items: string[], count?: number): TestState {
  return { items, count: count ?? items.length };
}

describe("UndoManager", () => {
  // ── Construction ──────────────────────────────────────────────────

  it("starts empty", () => {
    const mgr = new UndoManager<TestState>();
    expect(mgr.canUndo).toBe(false);
    expect(mgr.canRedo).toBe(false);
    expect(mgr.undoSize).toBe(0);
    expect(mgr.redoSize).toBe(0);
  });

  // ── Push ──────────────────────────────────────────────────────────

  it("push makes undo available", () => {
    const mgr = new UndoManager<TestState>();
    mgr.push(state(["a"]));
    expect(mgr.canUndo).toBe(true);
    expect(mgr.undoSize).toBe(1);
  });

  it("push clears the redo stack", () => {
    const mgr = new UndoManager<TestState>();
    mgr.push(state(["a"]));
    mgr.push(state(["a", "b"]));
    // undo to get a redo entry
    mgr.undo(state(["a", "b", "c"]));
    expect(mgr.canRedo).toBe(true);
    // new push should clear redo
    mgr.push(state(["a", "b", "d"]));
    expect(mgr.canRedo).toBe(false);
    expect(mgr.redoSize).toBe(0);
  });

  // ── Undo ──────────────────────────────────────────────────────────

  it("undo returns the previous snapshot", () => {
    const mgr = new UndoManager<TestState>();
    mgr.push(state(["a"]));
    const prev = mgr.undo(state(["a", "b"]));
    expect(prev).toEqual(state(["a"]));
  });

  it("undo on empty returns null", () => {
    const mgr = new UndoManager<TestState>();
    expect(mgr.undo(state(["x"]))).toBeNull();
  });

  it("undo creates a redo entry", () => {
    const mgr = new UndoManager<TestState>();
    mgr.push(state(["a"]));
    mgr.undo(state(["a", "b"]));
    expect(mgr.canRedo).toBe(true);
    expect(mgr.redoSize).toBe(1);
  });

  it("multiple undos walk backwards", () => {
    const mgr = new UndoManager<TestState>();
    mgr.push(state(["a"]));
    mgr.push(state(["a", "b"]));
    mgr.push(state(["a", "b", "c"]));

    const s3 = mgr.undo(state(["a", "b", "c", "d"]));
    expect(s3).toEqual(state(["a", "b", "c"]));

    const s2 = mgr.undo(state(["a", "b", "c"]));
    expect(s2).toEqual(state(["a", "b"]));

    const s1 = mgr.undo(state(["a", "b"]));
    expect(s1).toEqual(state(["a"]));

    expect(mgr.undo(state(["a"]))).toBeNull();
  });

  // ── Redo ──────────────────────────────────────────────────────────

  it("redo returns the undone snapshot", () => {
    const mgr = new UndoManager<TestState>();
    mgr.push(state(["a"]));
    mgr.undo(state(["a", "b"]));
    const redone = mgr.redo(state(["a"]));
    expect(redone).toEqual(state(["a", "b"]));
  });

  it("redo on empty returns null", () => {
    const mgr = new UndoManager<TestState>();
    expect(mgr.redo(state(["x"]))).toBeNull();
  });

  it("redo creates an undo entry", () => {
    const mgr = new UndoManager<TestState>();
    mgr.push(state(["a"]));
    mgr.undo(state(["a", "b"]));
    expect(mgr.undoSize).toBe(0);
    mgr.redo(state(["a"]));
    expect(mgr.undoSize).toBe(1);
  });

  // ── Undo + redo round-trip ────────────────────────────────────────

  it("undo then redo restores original state", () => {
    const mgr = new UndoManager<TestState>();
    const initial = state(["x", "y"]);
    const current = state(["x", "y", "z"]);
    mgr.push(initial);

    const undone = mgr.undo(current);
    expect(undone).toEqual(initial);

    const redone = mgr.redo(undone!);
    expect(redone).toEqual(current);
  });

  it("multiple undo-redo cycles are stable", () => {
    const mgr = new UndoManager<TestState>();
    mgr.push(state(["a"]));
    mgr.push(state(["b"]));

    // undo twice, redo twice, should end up at same place
    let cur = state(["c"]);
    cur = mgr.undo(cur)!; // -> b
    cur = mgr.undo(cur)!; // -> a
    cur = mgr.redo(cur)!; // -> b
    cur = mgr.redo(cur)!; // -> c
    expect(cur).toEqual(state(["c"]));
    expect(mgr.canRedo).toBe(false);
  });

  // ── Max size ──────────────────────────────────────────────────────

  it("caps the undo stack at maxSize", () => {
    const mgr = new UndoManager<TestState>(3);
    mgr.push(state(["a"]));
    mgr.push(state(["b"]));
    mgr.push(state(["c"]));
    mgr.push(state(["d"])); // should evict "a"
    expect(mgr.undoSize).toBe(3);

    // oldest entry ("a") is gone, so three undos should work
    const s1 = mgr.undo(state(["e"]));
    expect(s1).toEqual(state(["d"]));
    const s2 = mgr.undo(state(["d"]));
    expect(s2).toEqual(state(["c"]));
    const s3 = mgr.undo(state(["c"]));
    expect(s3).toEqual(state(["b"]));
    expect(mgr.undo(state(["b"]))).toBeNull();
  });

  // ── Clear ─────────────────────────────────────────────────────────

  it("clear drops all history", () => {
    const mgr = new UndoManager<TestState>();
    mgr.push(state(["a"]));
    mgr.push(state(["b"]));
    mgr.undo(state(["c"]));
    expect(mgr.canUndo).toBe(true);
    expect(mgr.canRedo).toBe(true);

    mgr.clear();
    expect(mgr.canUndo).toBe(false);
    expect(mgr.canRedo).toBe(false);
    expect(mgr.undoSize).toBe(0);
    expect(mgr.redoSize).toBe(0);
  });

  // ── Snapshot isolation ────────────────────────────────────────────

  it("pushed snapshots are independent of later mutations", () => {
    const mgr = new UndoManager<TestState>();
    const original = state(["a", "b"]);
    mgr.push(original);

    // Mutate the original object after pushing
    original.items.push("c");
    original.count = 3;

    const restored = mgr.undo(state(["x"]));
    expect(restored).toEqual(state(["a", "b"]));
    expect(restored!.items).toHaveLength(2);
  });

  it("undone snapshot is independent of later mutations", () => {
    const mgr = new UndoManager<TestState>();
    mgr.push(state(["a"]));
    const restored = mgr.undo(state(["a", "b"]));

    // Mutate the restored snapshot
    restored!.items.push("z");

    // Redo should still have the clean version
    const redone = mgr.redo(state(["a"]));
    expect(redone).toEqual(state(["a", "b"]));
    expect(redone!.items).toHaveLength(2);
  });

  // ── Edge cases ────────────────────────────────────────────────────

  it("works with maxSize of 1", () => {
    const mgr = new UndoManager<TestState>(1);
    mgr.push(state(["a"]));
    mgr.push(state(["b"])); // evicts "a"
    expect(mgr.undoSize).toBe(1);

    const s = mgr.undo(state(["c"]));
    expect(s).toEqual(state(["b"]));
    expect(mgr.undo(state(["b"]))).toBeNull();
  });

  it("default maxSize is 50", () => {
    const mgr = new UndoManager<TestState>();
    for (let i = 0; i < 60; i++) {
      mgr.push(state([`item-${i}`]));
    }
    expect(mgr.undoSize).toBe(50);
  });

  it("handles nested object snapshots", () => {
    interface Complex {
      pages: { key: string; rotation: number }[];
      annotations: { id: string; rect: { x: number; y: number } }[];
    }
    const mgr = new UndoManager<Complex>();
    mgr.push({
      pages: [{ key: "k1", rotation: 0 }],
      annotations: [{ id: "a1", rect: { x: 10, y: 20 } }],
    });

    const restored = mgr.undo({
      pages: [
        { key: "k1", rotation: 90 },
        { key: "k2", rotation: 0 },
      ],
      annotations: [],
    });
    expect(restored!.pages).toHaveLength(1);
    expect(restored!.pages[0].rotation).toBe(0);
    expect(restored!.annotations).toHaveLength(1);
    expect(restored!.annotations[0].rect).toEqual({ x: 10, y: 20 });
  });

  it("interleaved push-undo-push produces correct history", () => {
    const mgr = new UndoManager<TestState>();
    mgr.push(state(["a"])); // undo: [a]
    mgr.push(state(["b"])); // undo: [a, b]
    mgr.undo(state(["c"])); // undo: [a], redo: [c]
    mgr.push(state(["d"])); // undo: [a, d], redo: [] (cleared)

    expect(mgr.canRedo).toBe(false);
    expect(mgr.undoSize).toBe(2);

    const s = mgr.undo(state(["e"]));
    expect(s).toEqual(state(["d"]));
  });
});

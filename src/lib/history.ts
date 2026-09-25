/**
 * Generic undo/redo manager.  Stores deep-cloned snapshots of application
 * state so that mutations in the live state never corrupt the history.
 *
 * Usage (in App.svelte):
 *   const history = new UndoManager<Snapshot>(50);
 *   // Before a mutation:
 *   history.push(captureSnapshot());
 *   // Ctrl+Z:
 *   const prev = history.undo(captureSnapshot());
 *   if (prev) restoreSnapshot(prev);
 */

/** Deep-clone a value.  Uses `structuredClone` when available, falling back to
 *  the JSON round-trip (fine for plain data). */
function clone<T>(value: T): T {
  if (typeof structuredClone === "function") return structuredClone(value);
  return JSON.parse(JSON.stringify(value));
}

export class UndoManager<T> {
  private undoStack: T[] = [];
  private redoStack: T[] = [];
  private maxSize: number;

  constructor(maxSize = 50) {
    this.maxSize = maxSize;
  }

  /** Save the current state before a mutation. Clears the redo stack. */
  push(snapshot: T): void {
    this.undoStack.push(clone(snapshot));
    if (this.undoStack.length > this.maxSize) {
      this.undoStack.shift();
    }
    this.redoStack = [];
  }

  /** Revert to the previous state.  `current` is the live state at the time of
   *  the call — it's pushed onto the redo stack so the user can redo.
   *  Returns `null` when there's nothing to undo. */
  undo(current: T): T | null {
    if (this.undoStack.length === 0) return null;
    const prev = this.undoStack.pop()!;
    this.redoStack.push(clone(current));
    return prev;
  }

  /** Re-apply a previously undone action.  `current` is pushed onto the undo
   *  stack.  Returns `null` when there's nothing to redo. */
  redo(current: T): T | null {
    if (this.redoStack.length === 0) return null;
    const next = this.redoStack.pop()!;
    this.undoStack.push(clone(current));
    return next;
  }

  get canUndo(): boolean {
    return this.undoStack.length > 0;
  }

  get canRedo(): boolean {
    return this.redoStack.length > 0;
  }

  /** Number of undo steps available. */
  get undoSize(): number {
    return this.undoStack.length;
  }

  /** Number of redo steps available. */
  get redoSize(): number {
    return this.redoStack.length;
  }

  /** Drop all history (e.g. when a new document is opened). */
  clear(): void {
    this.undoStack = [];
    this.redoStack = [];
  }
}

// The material profile builder's screen-side logic: which step a session is
// on, how far it has come, and where a pattern's cells sit so a screen can
// show them as they lie on the plate. The server owns the session; this
// module only reads its view.
import type { CalibrationCell, CalibrationSession, CalibrationStep } from '../api';

/** The focus attribute: the one step that is a physical setup, cut one
 * ladder row per pass with the head re-focused between passes. */
export const FOCUS_ATTRIBUTE = 'OpenLaserManualFocus';

/** Whether `parameter` is the focus the operator sets by hand. */
export function isFocus(parameter: string | null): boolean {
  return parameter === FOCUS_ATTRIBUTE;
}

/** How far the session has come: every pick made of every step to tune. */
export function progress(session: CalibrationSession): { done: number; total: number } {
  return { done: session.picks.length, total: session.steps.length };
}

/** The step a session is on, `null` once every step has its pick. */
export function currentStep(session: CalibrationSession): CalibrationStep | null {
  return session.steps.find((step) => step.parameter === session.current) ?? null;
}

/** What a step's chip says: the pick once made, the bare name until then. */
export function stepLabel(step: CalibrationStep): string {
  return step.picked !== null ? `${step.name} · ${step.picked}` : step.name;
}

export type Placed = {
  /** The cell, as the server described it. */
  cell: CalibrationCell;
  /** The button's centre, in pixels inside the layout box. */
  x: number;
  y: number;
};

export type Layout = {
  /** The box's size, in pixels. */
  width: number;
  height: number;
  /** Every cell, positioned as its geometry lies on the plate, Y up. */
  placed: Placed[];
};

/** Places the pattern's cells as they lie on the plate: one button per
 * cell, centred on its geometry, spaced at least `target` pixels apart.
 * The plate's own coordinates run Y up, so the first cell of a ladder
 * sits lowest, the way the cut plate reads in front of the operator. */
export function cellLayout(cells: CalibrationCell[], target = 56): Layout {
  if (cells.length === 0) return { width: 0, height: 0, placed: [] };
  const xs = cells.map((cell) => cell.center[0]);
  const ys = cells.map((cell) => cell.center[1]);
  const minX = Math.min(...xs);
  const minY = Math.min(...ys);
  const spanX = Math.max(...xs) - minX;
  const spanY = Math.max(...ys) - minY;
  const spacing = (values: number[]): number => {
    const distinct = [...new Set(values)].sort((a, b) => a - b);
    return distinct.length < 2 ? Infinity : Math.min(...distinct.slice(1).map((v, i) => v - distinct[i]!));
  };
  const gap = Math.min(spacing(xs), spacing(ys));
  const scale = gap === Infinity ? 1 : target / gap;
  const place = (span: number): number => span * scale + target;
  return {
    width: place(spanX),
    height: place(spanY),
    placed: cells.map((cell) => ({
      cell,
      x: target / 2 + (cell.center[0] - minX) * scale,
      y: target / 2 + (cell.center[1] - minY) * scale,
    })),
  };
}

/** The Y a screen draws a cell at: the plate's own orientation, flipped
 * because a screen's Y runs down. */
export function screenY(layout: Layout, y: number): number {
  return layout.height - y;
}

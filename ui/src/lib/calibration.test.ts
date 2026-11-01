import { describe, expect, it } from 'vitest';
import { cellLayout, currentStep, isFocus, progress, screenY, stepLabel } from './calibration';
import type { CalibrationCell, CalibrationSession, CalibrationStep } from '../api';

const step = (parameter: string, picked: string | null = null): CalibrationStep =>
  ({ parameter, name: parameter === 'OpenLaserManualFocus' ? 'Focus' : 'Power', picked });

const cell = (label: string, x: number, y: number, value: number): CalibrationCell =>
  ({ label, layer: label, value, center: [x, y] });

const session = (over: Partial<CalibrationSession> = {}): CalibrationSession => ({
  recipe: { id: 'r1', name: 'Aluminum', laser: 'fiber', thickness_mm: 1, gas: 'N2' },
  current: 'OpenLaserManualFocus',
  steps: [step('OpenLaserManualFocus', '-1'), step('CutPower')],
  cells: [],
  picks: [{ parameter: 'OpenLaserManualFocus', value: '-1' }],
  complete: false,
  revision: 3,
  ...over,
});

describe('calibration session views', () => {
  it('knows the focus step from its attribute', () => {
    expect(isFocus('OpenLaserManualFocus')).toBe(true);
    expect(isFocus('CutPower')).toBe(false);
    expect(isFocus(null)).toBe(false);
  });
  it('counts the picks against the steps to tune', () => {
    expect(progress(session())).toEqual({ done: 1, total: 2 });
    expect(progress(session({ complete: true, picks: [], steps: [] }))).toEqual({ done: 0, total: 0 });
  });
  it('finds the current step, and none once complete', () => {
    expect(currentStep(session())?.name).toBe('Focus');
    expect(currentStep(session({ current: null, complete: true }))).toBeNull();
  });
  it('labels a step with its pick once made', () => {
    expect(stepLabel(step('CutPower'))).toBe('Power');
    expect(stepLabel(step('CutPower', '85'))).toBe('Power · 85');
  });
});

describe('calibration pattern layout', () => {
  it('places every cell at least a target apart, in plate orientation', () => {
    const ladder = [cell('F -5', 42.5, 5, -5), cell('F -4', 42.5, 20, -4), cell('F -3', 42.5, 35, -3)];
    const layout = cellLayout(ladder);
    expect(layout.width).toBe(56);
    expect(layout.height).toBeCloseTo(56 + 30 * (56 / 15), 6);
    // The first row sits lowest, and the rows step by the target spacing.
    expect(layout.placed[0]!.y).toBe(28);
    expect(layout.placed[1]!.y).toBeCloseTo(28 + 56, 6);
    expect(layout.placed[2]!.y).toBeCloseTo(28 + 112, 6);
    // A screen draws them top down: the last row is at the top.
    expect(screenY(layout, layout.placed[0]!.y)).toBeCloseTo(layout.height - 28, 6);
  });
  it('spreads a single row across and keeps one cell centred', () => {
    const row = [cell('Pw 55', 5, 20, 55), cell('Pw 85', 35, 20, 85), cell('Pw 95', 65, 20, 95)];
    const layout = cellLayout(row);
    expect(layout.height).toBe(56);
    expect(layout.placed.map((p) => p.y).every((y) => Math.abs(y - 28) < 1e-9)).toBe(true);
    expect(layout.width).toBeCloseTo(56 + 60 * (56 / 30), 6);
  });
  it('handles a lone cell and an empty pattern', () => {
    expect(cellLayout([])).toEqual({ width: 0, height: 0, placed: [] });
    const lone = cellLayout([cell('F -2', 7.5, 5, -2)]);
    expect(lone.width).toBe(56);
    expect(lone.height).toBe(56);
    expect(lone.placed[0]!.x).toBe(28);
  });
});

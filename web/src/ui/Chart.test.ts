import { describe, expect, it } from 'vitest';
import { toRuns, type Point } from './Chart';

describe('toRuns', () => {
  const at = (hours: number, value: number): Point => ({
    t: new Date(`2026-08-20T${String(hours).padStart(2, '0')}:00:00Z`).getTime(),
    value,
  });

  it('keeps a continuous series in one piece', () => {
    const runs = toRuns([at(1, 10), at(2, 11), at(3, 12)], 3 * 3600);
    expect(runs).toHaveLength(1);
    expect(runs[0]).toHaveLength(3);
  });

  it('breaks the line where nothing was measured', () => {
    // Joining across the gap would draw a measurement that never happened.
    const runs = toRuns([at(1, 10), at(2, 11), at(20, 4)], 3 * 3600);
    expect(runs).toHaveLength(2);
    expect(runs[0]).toHaveLength(2);
    expect(runs[1]).toHaveLength(1);
  });

  it('sorts before splitting, so order of arrival does not matter', () => {
    const runs = toRuns([at(3, 12), at(1, 10), at(2, 11)], 3 * 3600);
    expect(runs[0]?.map((point) => point.value)).toEqual([10, 11, 12]);
  });
});

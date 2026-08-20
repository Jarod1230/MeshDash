import { describe, expect, it } from 'vitest';
import { describeRoute } from './types';

describe('describeRoute', () => {
  it('separates "no route" from "nothing in between"', () => {
    // The firmware marks an unknown route with OUT_PATH_UNKNOWN; treating it
    // as zero stations turns an unreachable node into the nearest one.
    expect(describeRoute(null)).toBe('Weg unbekannt');
    expect(describeRoute(0)).toBe('direkt');
  });

  it('counts stations, which is what an operator reads', () => {
    expect(describeRoute(1)).toBe('1 Station');
    expect(describeRoute(3)).toBe('3 Stationen');
  });
});

import { describe, expect, it } from 'vitest';
import { formatDistance, positioned, project, scaleStep } from './Map';
import type { KnownContact } from './types';

const at = (name: string, latitude: number | null, longitude: number | null): KnownContact => ({
  public_key: name.repeat(32).slice(0, 64),
  name,
  contact_type: 2,
  flags: 0,
  path: '',
  stations: 0,
  latitude,
  longitude,
  last_advert: 0,
  first_seen: new Date().toISOString(),
  last_seen: new Date().toISOString(),
});

describe('positioned', () => {
  it('leaves out a node that reports no position', () => {
    expect(positioned([at('a', null, null), at('b', 52.5, 13.4)])).toHaveLength(1);
  });

  it('treats zero as unset rather than as a place', () => {
    // The firmware stores 0/0 for "not set". Drawing it would put nodes in
    // the Gulf of Guinea.
    expect(positioned([at('a', 0, 0)])).toHaveLength(0);
  });
});

describe('project', () => {
  const now = Date.now();

  it('has nothing to draw when nobody reports a position', () => {
    expect(project([at('a', null, null)], now)).toBeNull();
  });

  it('puts north up', () => {
    const projection = project([at('sued', 52.4, 13.4), at('nord', 52.6, 13.4)], now);
    const north = projection?.placed.find((node) => node.contact.name === 'nord');
    const south = projection?.placed.find((node) => node.contact.name === 'sued');

    expect(north!.y).toBeLessThan(south!.y);
  });

  it('keeps one scale for both axes, so the bar means something', () => {
    // Two nodes 0.1° apart in latitude and 0.1° apart in longitude are not
    // the same distance apart on the ground — at 52° the longitude gap is
    // about 61 percent of the latitude one, and the drawing must show that.
    const projection = project(
      [at('mitte', 52.5, 13.4), at('nord', 52.6, 13.4), at('ost', 52.5, 13.5)],
      now,
    );
    const centre = projection!.placed.find((n) => n.contact.name === 'mitte')!;
    const north = projection!.placed.find((n) => n.contact.name === 'nord')!;
    const east = projection!.placed.find((n) => n.contact.name === 'ost')!;

    const northPixels = Math.abs(centre.y - north.y);
    const eastPixels = Math.abs(centre.x - east.x);

    expect(eastPixels / northPixels).toBeCloseTo(Math.cos((52.55 * Math.PI) / 180), 2);
  });

  it('survives a single node without dividing by zero', () => {
    const projection = project([at('allein', 52.5, 13.4)], now);

    expect(projection?.placed).toHaveLength(1);
    expect(Number.isFinite(projection!.pixelsPerMetre)).toBe(true);
    expect(projection!.spanMetres).toBe(1_000);
  });

  it('marks a node not heard in a day as stale', () => {
    const old = at('alt', 52.5, 13.4);
    const stale = {
      ...old,
      last_seen: new Date(now - 26 * 3600 * 1000).toISOString(),
    };

    expect(project([stale], now)!.placed[0]!.stale).toBe(true);
  });
});

describe('scaleStep', () => {
  it('picks a round number that fits inside the drawing', () => {
    expect(scaleStep(40_000)).toBe(10_000);
    expect(scaleStep(4_000)).toBe(1_000);
    expect(scaleStep(900)).toBe(200);
  });

  it('never returns a step larger than a quarter of the span', () => {
    for (const span of [1_000, 3_500, 12_000, 87_000, 250_000]) {
      expect(scaleStep(span)).toBeLessThanOrEqual(span / 4);
    }
  });
});

describe('formatDistance', () => {
  it('switches to kilometres where metres get unwieldy', () => {
    expect(formatDistance(750)).toBe('750 m');
    expect(formatDistance(1_000)).toBe('1 km');
    expect(formatDistance(12_500)).toBe('12,5 km');
  });
});

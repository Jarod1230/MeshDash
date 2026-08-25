import { describe, expect, it } from 'vitest';
import { place } from './Topology';
import type { KnownContact } from './types';

const contact = (name: string, stations: number | null, minutesAgo: number): KnownContact => ({
  public_key: name.repeat(8).slice(0, 64),
  name,
  contact_type: 2,
  flags: 0,
  position_source: null,
  reported_latitude: null,
  reported_longitude: null,
  path: stations === null ? null : '',
  stations,
  latitude: null,
  longitude: null,
  last_advert: 0,
  first_seen: new Date().toISOString(),
  last_seen: new Date(Date.now() - minutesAgo * 60_000).toISOString(),
});

describe('place', () => {
  const now = Date.now();

  it('puts a direct neighbour on the innermost ring', () => {
    // No hops in between means one step away, not zero.
    const [placed] = place([contact('a', 0, 1)], now);
    expect(placed?.hops).toBe(1);
  });

  it('moves a node outward for every station on its route', () => {
    const placed = place([contact('a', 0, 1), contact('b', 2, 1)], now);
    expect(placed.find((node) => node.contact.name === 'b')?.hops).toBe(3);
  });

  it('marks a node not heard in a day as stale rather than dropping it', () => {
    // "Was here, is not answering" is the thing an operator looks for.
    const [placed] = place([contact('a', 0, 60 * 25)], now);
    expect(placed?.stale).toBe(true);
  });

  it('caps the outermost ring so a long route stays on the canvas', () => {
    const [placed] = place([contact('a', 8, 1)], now);
    expect(placed?.hops).toBe(4);
  });
});

describe('place with no known route', () => {
  const now = Date.now();

  it('puts a contact without a route on the outermost ring, not the innermost', () => {
    // Before this, "no route" and "direct neighbour" were both zero hops, so
    // an unreachable node was drawn as the closest one.
    const [placed] = place([contact('unerreichbar', null, 1)], now);

    expect(placed?.hops).toBe(4);
  });

  it('keeps a real direct neighbour on the innermost ring', () => {
    const [placed] = place([contact('direkt', 0, 1)], now);

    expect(placed?.hops).toBe(1);
  });
});

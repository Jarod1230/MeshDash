import { describe, expect, it } from 'vitest';
import { place } from './Topology';
import type { KnownContact } from './types';

const contact = (name: string, path: string, minutesAgo: number): KnownContact => ({
  public_key: name.repeat(8).slice(0, 64),
  name,
  contact_type: 2,
  flags: 0,
  path,
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
    const [placed] = place([contact('a', '', 1)], now);
    expect(placed?.hops).toBe(1);
  });

  it('moves a node outward for every hop in its path', () => {
    const placed = place([contact('a', '', 1), contact('b', '0102', 1)], now);
    expect(placed.find((node) => node.contact.name === 'b')?.hops).toBe(3);
  });

  it('marks a node not heard in a day as stale rather than dropping it', () => {
    // "Was here, is not answering" is the thing an operator looks for.
    const [placed] = place([contact('a', '', 60 * 25)], now);
    expect(placed?.stale).toBe(true);
  });

  it('caps the outermost ring so a long path stays on the canvas', () => {
    const [placed] = place([contact('a', '0102030405060708', 1)], now);
    expect(placed?.hops).toBe(4);
  });
});

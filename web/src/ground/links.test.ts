import { describe, expect, it } from 'vitest';
import { links, resolve, strokeFor } from './links';
import type { GroundNode } from './projection';
import type { Trace } from '../modules/nodes/types';

const NOW = Date.parse('2026-08-27T12:00:00Z');

function node(key: string, overrides: Partial<GroundNode> = {}): GroundNode {
  return {
    key,
    name: key,
    latitude: 54,
    longitude: 13,
    stations: 0,
    lastSeen: NOW,
    own: false,
    source: 'advert',
    ...overrides,
  };
}

function trace(overrides: Partial<Trace> = {}): Trace {
  return {
    id: 1,
    public_key: 'cc'.repeat(32),
    asked_at: new Date(NOW - 60_000).toISOString(),
    answered_at: new Date(NOW - 59_000).toISOString(),
    final_snr: 4,
    hops: [],
    ...overrides,
  };
}

const OWN = 'aa'.repeat(32);

describe('resolve', () => {
  const nodes = [node('ab' + 'cd'.repeat(31)), node('ef' + '01'.repeat(31))];

  it('names a station when only one node can be meant', () => {
    expect(resolve('ab', nodes)).toBe(nodes[0]?.key);
  });

  it('names nobody when the prefix fits more than one', () => {
    // The same birthday problem as a message's sender prefix. A line drawn
    // from a coin toss looks exactly like a measured one.
    const twins = [node('ab' + '11'.repeat(31)), node('ab' + '22'.repeat(31))];

    expect(resolve('ab', twins)).toBeNull();
  });

  it('names nobody for a prefix that fits nothing', () => {
    expect(resolve('99', nodes)).toBeNull();
    expect(resolve('', nodes)).toBeNull();
  });
});

describe('links', () => {
  it('draws a line to every direct neighbour, and says nothing about quality', () => {
    const found = links(
      [node(OWN, { own: true }), node('bb'.repeat(32), { stations: 0 })],
      [],
      NOW,
    );

    expect(found).toHaveLength(1);
    expect(found[0]?.kind).toBe('direkt');
    // Certain that it exists, unmeasured how well it carries.
    expect(found[0]?.snr).toBeNull();
  });

  it('leaves a contact behind a station alone until something measures it', () => {
    // Its route is a list of one-byte prefixes. A line from that would put a
    // measurement's weight behind a guess.
    const found = links(
      [node(OWN, { own: true }), node('bb'.repeat(32), { stations: 2 })],
      [],
      NOW,
    );

    expect(found).toEqual([]);
  });

  it('walks a traced route leg by leg, with the quality each station reported', () => {
    const middle = 'bb' + '77'.repeat(31);
    const target = 'cc'.repeat(32);
    const found = links(
      [node(OWN, { own: true }), node(middle, { stations: 1 }), node(target, { stations: 1 })],
      [trace({ public_key: target, hops: [{ key_prefix: 'bb', snr: -4 }] })],
      NOW,
    );

    expect(found).toHaveLength(2);
    const first = found.find((link) => link.to === middle || link.from === middle);
    // The station's own value says how well it heard the leg arriving there.
    expect(found.find((link) => link.id.includes(OWN))?.snr).toBe(-4);
    // The last leg has no station of its own, so no measurement is claimed.
    expect(found.find((link) => link.id.includes(target))?.snr).toBeNull();
    expect(first?.kind).toBe('verfolgt');
  });

  it('drops only the legs a nameless station touches', () => {
    const known = 'dd' + '55'.repeat(31);
    const target = 'cc'.repeat(32);
    const found = links(
      [node(OWN, { own: true }), node(known, { stations: 2 }), node(target, { stations: 2 })],
      [
        trace({
          public_key: target,
          // The first station matches nothing known; the second is clear.
          hops: [
            { key_prefix: '99', snr: -2 },
            { key_prefix: 'dd', snr: -6 },
          ],
        }),
      ],
      NOW,
    );

    // Only the leg between the named station and the target survives.
    expect(found).toHaveLength(1);
    expect(found[0]?.id).toBe([known, target].sort().join('|'));
  });

  it('lets a measurement outrank a bare neighbour, whichever came first', () => {
    const neighbour = 'bb'.repeat(32);
    const target = 'cc'.repeat(32);
    const found = links(
      [
        node(OWN, { own: true }),
        node(neighbour, { stations: 0 }),
        node(target, { stations: 1 }),
      ],
      [trace({ public_key: target, hops: [{ key_prefix: 'bb', snr: -1 }] })],
      NOW,
    );

    const own_to_neighbour = found.find((link) => link.id === [OWN, neighbour].sort().join('|'));
    expect(own_to_neighbour?.snr).toBe(-1);
  });

  it('ignores a trace that never came back', () => {
    const found = links(
      [node(OWN, { own: true }), node('cc'.repeat(32), { stations: 1 })],
      [trace({ answered_at: null, hops: [] })],
      NOW,
    );

    expect(found).toEqual([]);
  });
});

describe('strokeFor', () => {
  it('gives the unmeasured the thinnest stroke there is', () => {
    // So that "we know it exists" never looks like "we measured it and it is
    // good".
    expect(strokeFor(null)).toBeLessThan(strokeFor(-20));
  });

  it('grows with the measurement and stops at both ends', () => {
    expect(strokeFor(-30)).toBe(strokeFor(-10));
    expect(strokeFor(20)).toBe(strokeFor(5));
    expect(strokeFor(0)).toBeGreaterThan(strokeFor(-8));
  });
});

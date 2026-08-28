import { describe, expect, it } from 'vitest';
import { neighbours, type HeardBy } from './neighbours';
import type { Named } from './prefix';
import type { Trace } from '../modules/nodes/types';

const NOW = Date.parse('2026-08-29T12:00:00Z');
const OWN = '99'.repeat(32);
const BRIDGE = 'fb' + '07'.repeat(31);
const HOME = 'd7' + '95'.repeat(31);

const NODES: Named[] = [
  { key: OWN, name: 'eigener', own: true },
  { key: BRIDGE, name: 'Brücke', own: false },
  { key: HOME, name: 'Zuhause', own: false },
];

function overheard(overrides: Partial<HeardBy> = {}): HeardBy {
  return {
    talker: 'fb',
    listener: '',
    width: 1,
    first_seen: new Date(NOW - 7_200_000).toISOString(),
    last_seen: new Date(NOW - 60_000).toISOString(),
    heard: 12,
    ...overrides,
  };
}

function trace(overrides: Partial<Trace> = {}): Trace {
  return {
    id: 1,
    public_key: HOME,
    asked_at: new Date(NOW - 3_600_000).toISOString(),
    answered_at: new Date(NOW - 3_599_000).toISOString(),
    final_snr: 4,
    hops: [],
    ...overrides,
  };
}

describe('neighbours', () => {
  it('separates who hears whom, from the asked node’s side', () => {
    const found = neighbours(OWN, NODES, [], [overheard({ talker: 'fb', listener: '' })]);

    expect(found).toHaveLength(1);
    expect(found[0]?.key).toBe(BRIDGE);
    // This node heard the bridge; nothing says the bridge heard this node.
    expect(found[0]?.hears?.heard).toBe(12);
    expect(found[0]?.heardBy).toBeNull();
  });

  it('turns the same pair round when asked from the other side', () => {
    const found = neighbours(BRIDGE, NODES, [], [overheard({ talker: 'fb', listener: '' })]);

    expect(found[0]?.key).toBe(OWN);
    expect(found[0]?.heardBy?.heard).toBe(12);
    expect(found[0]?.hears).toBeNull();
  });

  it('adds up one direction seen under different prefix widths', () => {
    const found = neighbours(
      OWN,
      NODES,
      [],
      [overheard({ talker: 'fb', width: 1, heard: 12 }), overheard({ talker: 'fb07', width: 2, heard: 3 })],
    );

    expect(found[0]?.hears?.heard).toBe(15);
    // The wider prefix is the stronger claim, so that is what is reported.
    expect(found[0]?.hears?.width).toBe(2);
  });

  it('keeps measurements apart from sightings rather than scoring them together', () => {
    const found = neighbours(
      OWN,
      NODES,
      [trace({ public_key: HOME, hops: [{ key_prefix: 'fb', snr: -4 }] })],
      [overheard()],
    );

    expect(found[0]?.measured).toEqual([{ snr: -4, at: NOW - 3_600_000 }]);
    expect(found[0]?.hears?.heard).toBe(12);
  });

  it('finds a neighbour that only a measurement knows about', () => {
    const found = neighbours(BRIDGE, NODES, [trace({ public_key: HOME, hops: [{ key_prefix: 'fb', snr: -4 }] })], []);

    // Own → bridge → home: the bridge has two measured neighbours and no
    // overheard ones.
    expect(found.map((one) => one.key).sort()).toEqual([HOME, OWN].sort());
  });

  it('names nobody for a prefix that fits two nodes', () => {
    const twins: Named[] = [
      { key: OWN, name: 'eigener', own: true },
      { key: 'fb' + '11'.repeat(31), name: 'eins', own: false },
      { key: 'fb' + '22'.repeat(31), name: 'zwei', own: false },
    ];

    expect(neighbours(OWN, twins, [], [overheard()])).toEqual([]);
  });

  it('puts the neighbour with the most behind it first', () => {
    const found = neighbours(
      OWN,
      NODES,
      [],
      [overheard({ talker: 'fb', heard: 2 }), overheard({ talker: 'd7', heard: 40 })],
    );

    expect(found.map((one) => one.name)).toEqual(['Zuhause', 'Brücke']);
  });
});

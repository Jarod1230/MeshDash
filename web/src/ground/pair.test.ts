import { describe, expect, it } from 'vitest';
import { facts, pairId } from './pair';
import type { HeardBy } from './links';
import type { GroundNode } from './projection';
import type { Trace } from '../modules/nodes/types';

const NOW = Date.parse('2026-08-29T12:00:00Z');
const OWN = '99'.repeat(32);
const BRIDGE = 'fb' + '07'.repeat(31);
const HOME = 'd7' + '95'.repeat(31);

function node(key: string, overrides: Partial<GroundNode> = {}): GroundNode {
  return {
    key,
    name: key.slice(0, 4),
    latitude: 54,
    longitude: 13,
    stations: null,
    lastSeen: NOW,
    own: false,
    source: 'advert',
    ...overrides,
  };
}

const NODES = [node(OWN, { own: true, stations: 0 }), node(BRIDGE), node(HOME)];

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

describe('facts', () => {
  it('says nothing about a pair whose ends it does not know', () => {
    expect(facts(pairId(OWN, 'ab'.repeat(32)), NODES, [], [])).toBeNull();
  });

  it('keeps the two directions of hearing apart', () => {
    // A hears B and B does not hear A happens constantly with LoRa, and it is
    // usually the finding somebody is after. One line hides it.
    const found = facts(
      pairId(OWN, BRIDGE),
      NODES,
      [],
      [overheard({ talker: 'fb', listener: '' })],
    );

    expect(found?.overheard).toHaveLength(1);
    expect(found?.overheard[0]?.talker).toBe(BRIDGE);
    expect(found?.overheard[0]?.listener).toBe(OWN);
    expect(found?.overheard[0]?.heard).toBe(12);
  });

  it('adds up the same direction seen under different prefix widths', () => {
    // The same node written with one byte by one sender and two by another is
    // one hearing, and the counts belong together.
    const found = facts(
      pairId(OWN, BRIDGE),
      NODES,
      [],
      [
        overheard({ talker: 'fb', width: 1, heard: 12 }),
        overheard({ talker: 'fb07', width: 2, heard: 3 }),
      ],
    );

    expect(found?.overheard).toHaveLength(1);
    expect(found?.overheard[0]?.heard).toBe(15);
    // The wider prefix is the stronger claim, so that is the one reported.
    expect(found?.overheard[0]?.width).toBe(2);
  });

  it('reports both directions when both were seen', () => {
    const found = facts(
      pairId(BRIDGE, HOME),
      NODES,
      [],
      [
        overheard({ talker: 'fb', listener: 'd7', heard: 4 }),
        overheard({ talker: 'd7', listener: 'fb', heard: 1 }),
      ],
    );

    expect(found?.overheard).toHaveLength(2);
  });

  it('picks out the legs a trace measured between exactly these two', () => {
    const found = facts(
      pairId(OWN, BRIDGE),
      NODES,
      [trace({ public_key: HOME, hops: [{ key_prefix: 'fb', snr: -4 }] })],
      [],
    );

    expect(found?.measured).toHaveLength(1);
    expect(found?.measured[0]?.snr).toBe(-4);
  });

  it('claims no measurement for the leg a trace had no station for', () => {
    // The last leg has no station of its own, and final_snr describes the
    // answer coming back rather than that leg.
    const found = facts(
      pairId(BRIDGE, HOME),
      NODES,
      [trace({ public_key: HOME, hops: [{ key_prefix: 'fb', snr: -4 }] })],
      [],
    );

    expect(found?.measured).toHaveLength(1);
    expect(found?.measured[0]?.snr).toBeNull();
  });

  it('knows a direct neighbour of this node when it sees one', () => {
    const direct = [node(OWN, { own: true }), node(BRIDGE, { stations: 0 })];

    expect(facts(pairId(OWN, BRIDGE), direct, [], [])?.direct).toBe(true);
    // Two other nodes being neighbours of each other is not something this
    // node can say.
    expect(facts(pairId(BRIDGE, HOME), NODES, [], [])?.direct).toBe(false);
  });

  it('ignores a trace that never came back', () => {
    const found = facts(
      pairId(OWN, BRIDGE),
      NODES,
      [trace({ answered_at: null, hops: [{ key_prefix: 'fb', snr: -4 }] })],
      [],
    );

    expect(found?.measured).toEqual([]);
  });
});

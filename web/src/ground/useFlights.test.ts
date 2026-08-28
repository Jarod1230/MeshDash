import { describe, expect, it } from 'vitest';
import { follow, positionOf, type Flight } from './useFlights';
import { toWorld, type GroundNode } from './projection';

const NOW = Date.parse('2026-08-28T12:00:00Z');

function node(key: string, overrides: Partial<GroundNode> = {}): GroundNode {
  return {
    key,
    name: key.slice(0, 4),
    latitude: 54,
    longitude: 13,
    stations: 0,
    lastSeen: NOW,
    own: false,
    source: 'advert',
    ...overrides,
  };
}

const OWN = node('99'.repeat(32), { own: true, latitude: 54.0, longitude: 13.0 });
const BRIDGE = node('fb' + '07'.repeat(31), { latitude: 54.01, longitude: 13.01 });
const HOME = node('d7' + '95'.repeat(31), { latitude: 54.02, longitude: 13.02 });

describe('follow', () => {
  it('walks the stations and ends at this node', () => {
    const legs = follow(['fb', 'd7'], [OWN, BRIDGE, HOME]);

    expect(legs).toHaveLength(3);
    expect(legs[0]).toEqual(toWorld(54.01, 13.01));
    expect(legs[2]).toEqual(toWorld(54.0, 13.0));
  });

  it('keeps only the run that reaches this node', () => {
    // The first station fits nothing known. What is before the break cannot
    // be drawn; what comes after it still can.
    const legs = follow(['55', 'fb'], [OWN, BRIDGE]);

    expect(legs).toHaveLength(2);
    expect(legs[0]).toEqual(toWorld(54.01, 13.01));
  });

  it('draws nothing for a packet heard straight from its sender', () => {
    // An empty path means nobody forwarded it. The sender is named only
    // inside the encrypted payload, so there is no journey to draw.
    expect(follow([], [OWN, BRIDGE])).toEqual([]);
  });

  it('draws nothing when the prefix could be either of two nodes', () => {
    const twins = [node('fb' + '11'.repeat(31)), node('fb' + '22'.repeat(31))];

    expect(follow(['fb'], [OWN, ...twins])).toEqual([]);
  });

  it('draws nothing while this node has no position of its own', () => {
    // Every flight ends here. Without a place for "here" there is no line to
    // end on, and inventing one would put traffic where it never was.
    const nowhere = node('99'.repeat(32), { own: true, latitude: null, longitude: null });

    expect(follow(['fb'], [nowhere, BRIDGE])).toEqual([]);
  });

  it('skips a station that is known but reports no position', () => {
    const unplaced = node('fb' + '07'.repeat(31), { latitude: null, longitude: null });

    expect(follow(['fb'], [OWN, unplaced])).toEqual([]);
  });
});

describe('positionOf', () => {
  const flight: Flight = {
    id: 1,
    legs: [toWorld(54.0, 13.0), toWorld(54.0, 13.02), toWorld(54.0, 13.04)],
    startedAt: NOW,
    duration: 1_000,
    payloadType: 2,
  };

  it('starts where it set off and moves on', () => {
    expect(positionOf(flight, NOW)?.x).toBeCloseTo(flight.legs[0]!.x, 12);
    expect(positionOf(flight, NOW + 250)!.x).toBeGreaterThan(flight.legs[0]!.x);
  });

  it('is halfway along at half the time, which is the second station', () => {
    expect(positionOf(flight, NOW + 500)?.x).toBeCloseTo(flight.legs[1]!.x, 12);
  });

  it('is gone once it has arrived', () => {
    // Drawn as arrived rather than parked on the last station: a dot sitting
    // there would read as a node.
    expect(positionOf(flight, NOW + 1_000)).toBeNull();
    expect(positionOf(flight, NOW + 5_000)).toBeNull();
  });
});

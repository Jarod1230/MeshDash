import { describe, expect, it } from 'vitest';
import {
  fit,
  formatDistance,
  geography,
  isPlaceable,
  onScreen,
  rings,
  scaleStep,
  zoomAt,
  type GroundNode,
} from './projection';

const NOW = Date.parse('2026-08-26T12:00:00Z');

function node(overrides: Partial<GroundNode> = {}): GroundNode {
  return {
    key: 'aa'.repeat(32),
    name: 'Knoten',
    latitude: null,
    longitude: null,
    stations: 0,
    lastSeen: NOW,
    own: false,
    source: null,
    ...overrides,
  };
}

describe('isPlaceable', () => {
  it('does not take the firmware’s "unset" for a spot in the Gulf of Guinea', () => {
    expect(isPlaceable(node({ latitude: 0, longitude: 0 }))).toBe(false);
    expect(isPlaceable(node({ latitude: 54.33, longitude: 13.07 }))).toBe(true);
    expect(isPlaceable(node({ latitude: 54.33, longitude: null }))).toBe(false);
  });
});

describe('geography', () => {
  it('refuses to call one point a geography', () => {
    expect(geography([node({ latitude: 54.33, longitude: 13.07 })], NOW)).toBeNull();
  });

  it('places nodes in metres around their common centre', () => {
    const geo = geography(
      [
        node({ key: 'a', latitude: 54.0, longitude: 13.0 }),
        node({ key: 'b', latitude: 54.02, longitude: 13.0 }),
      ],
      NOW,
    );

    expect(geo).not.toBeNull();
    expect(geo?.anchor.latitude).toBeCloseTo(54.01, 6);
    // Two hundredths of a degree of latitude is a bit over two kilometres.
    expect(geo?.spanMetres).toBeGreaterThan(2_000);
    expect(geo?.spanMetres).toBeLessThan(2_500);
    // Symmetric about the centre: one north of it, one south.
    const norths = (geo?.placed ?? []).map((placed) => placed.north);
    expect((norths[0] ?? 0) + (norths[1] ?? 0)).toBeCloseTo(0, 6);
  });

  it('keeps a scale even when every node sits on the same spot', () => {
    const geo = geography(
      [
        node({ key: 'a', latitude: 54.0, longitude: 13.0 }),
        node({ key: 'b', latitude: 54.0, longitude: 13.0 }),
      ],
      NOW,
    );

    // Not zero, which would divide the scale bar by nothing.
    expect(geo?.spanMetres).toBe(1_000);
  });

  it('fades what has not been heard for a day instead of dropping it', () => {
    const geo = geography(
      [
        node({ key: 'a', latitude: 54.0, longitude: 13.0, lastSeen: NOW - 90_000_000 }),
        node({ key: 'b', latitude: 54.02, longitude: 13.0 }),
      ],
      NOW,
    );

    expect(geo?.placed).toHaveLength(2);
    expect(geo?.placed[0]?.stale).toBe(true);
    expect(geo?.placed[1]?.stale).toBe(false);
  });
});

describe('fit and onScreen', () => {
  const geo = geography(
    [
      node({ key: 'a', latitude: 54.0, longitude: 13.0 }),
      node({ key: 'b', latitude: 54.02, longitude: 13.04 }),
    ],
    NOW,
  );

  it('brings everything inside the padding', () => {
    const view = fit(geo!, 800, 600, 60);

    for (const placed of geo!.placed) {
      const { x, y } = onScreen(placed, view, 800, 600);
      expect(x).toBeGreaterThanOrEqual(60 - 1);
      expect(x).toBeLessThanOrEqual(800 - 60 + 1);
      expect(y).toBeGreaterThanOrEqual(60 - 1);
      expect(y).toBeLessThanOrEqual(600 - 60 + 1);
    }
  });

  it('puts north up', () => {
    const view = fit(geo!, 800, 600, 60);
    const northern = geo!.placed.find((placed) => placed.node.key === 'b')!;
    const southern = geo!.placed.find((placed) => placed.node.key === 'a')!;

    expect(onScreen(northern, view, 800, 600).y).toBeLessThan(
      onScreen(southern, view, 800, 600).y,
    );
  });
});

describe('zoomAt', () => {
  it('keeps what is under the cursor under the cursor', () => {
    const view = { east: 0, north: 0, metresPerPixel: 10 };
    const placed = { node: node(), east: 1_000, north: 500, stale: false };
    const before = onScreen(placed, view, 800, 600);

    const zoomed = zoomAt(view, 0.5, before.x, before.y, 800, 600);
    const after = onScreen(placed, zoomed, 800, 600);

    expect(after.x).toBeCloseTo(before.x, 6);
    expect(after.y).toBeCloseTo(before.y, 6);
  });

  it('stops before a pixel means less than a reported coordinate does', () => {
    const view = { east: 0, north: 0, metresPerPixel: 0.02 };

    expect(zoomAt(view, 0.1, 400, 300, 800, 600).metresPerPixel).toBe(0.01);
    expect(zoomAt({ ...view, metresPerPixel: 900 }, 10, 400, 300, 800, 600).metresPerPixel).toBe(
      1_000,
    );
  });
});

describe('scaleStep', () => {
  it('picks a round number that fits', () => {
    expect(scaleStep(4_000)).toBe(1_000);
    expect(scaleStep(9_000)).toBe(2_000);
    expect(scaleStep(1_000)).toBe(200);
  });
});

describe('formatDistance', () => {
  it('says metres below a kilometre and kilometres above', () => {
    expect(formatDistance(240)).toBe('240 m');
    expect(formatDistance(2_400)).toBe('2,4 km');
  });
});

describe('rings', () => {
  it('measures the distance from the centre in hops, not in guesses', () => {
    const placed = rings(
      [
        node({ key: 'near', stations: 0 }),
        node({ key: 'far', stations: 2 }),
        node({ key: 'unknown', stations: null }),
      ],
      NOW,
    );

    expect(placed.find((one) => one.node.key === 'near')?.hops).toBe(1);
    expect(placed.find((one) => one.node.key === 'far')?.hops).toBe(3);
    // No known route is not zero hops away — it goes to the outer edge.
    expect(placed.find((one) => one.node.key === 'unknown')?.hops).toBe(4);
    expect(placed.find((one) => one.node.key === 'unknown')?.radius).toBe(1);
  });

  it('leaves this node out, because it is the centre', () => {
    const placed = rings([node({ key: 'self', own: true }), node({ key: 'other' })], NOW);

    expect(placed.map((one) => one.node.key)).toEqual(['other']);
  });
});

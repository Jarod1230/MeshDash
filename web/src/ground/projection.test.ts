import { describe, expect, it } from 'vitest';
import {
  TILE_SIZE,
  fit,
  formatDistance,
  geography,
  heard,
  isPlaceable,
  metresPerPixel,
  onScreen,
  panBy,
  rings,
  scaleStep,
  toLatitude,
  toWorld,
  visibleTiles,
  zoomAt,
  type GroundNode,
} from './projection';

const NOW = Date.parse('2026-08-27T12:00:00Z');

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

describe('toWorld', () => {
  it('puts the origin where every tile scheme puts it', () => {
    // Null Island sits in the middle of the square; the north-west corner is
    // the origin, which is what tile numbering counts from.
    expect(toWorld(0, 0)).toEqual({ x: 0.5, y: 0.5 });
    expect(toWorld(0, -180).x).toBeCloseTo(0, 12);
    expect(toWorld(85.05112878, 0).y).toBeCloseTo(0, 6);
  });

  it('agrees with a tile number anyone can look up', () => {
    // Berlin at zoom 10 is tile 550/335 in every raster scheme there is.
    const world = toWorld(52.520008, 13.404954);

    expect(Math.floor(world.x * 2 ** 10)).toBe(550);
    expect(Math.floor(world.y * 2 ** 10)).toBe(335);
  });

  it('comes back to the latitude it started from', () => {
    expect(toLatitude(toWorld(54.331026, 13.070254).y)).toBeCloseTo(54.331026, 9);
  });
});

describe('heard', () => {
  it('keeps the middle state, which is the interesting one', () => {
    // "Was answering an hour ago and is quiet now" is neither fine nor gone,
    // and it is exactly the moment somebody wants to see.
    expect(heard(NOW - 60_000, NOW)).toBe('jetzt');
    expect(heard(NOW - 3 * 3_600_000, NOW)).toBe('still');
    expect(heard(NOW - 3 * 86_400_000, NOW)).toBe('lange');
  });

  it('draws the line at the hour and at the day', () => {
    expect(heard(NOW - 3_599_000, NOW)).toBe('jetzt');
    expect(heard(NOW - 3_601_000, NOW)).toBe('still');
    expect(heard(NOW - 86_399_000, NOW)).toBe('still');
    expect(heard(NOW - 86_401_000, NOW)).toBe('lange');
  });
});

describe('geography', () => {
  it('refuses to call one point a geography', () => {
    expect(geography([node({ latitude: 54.33, longitude: 13.07 })], NOW)).toBeNull();
  });

  it('reports the latitude in the middle, where the scale bar is true', () => {
    const geo = geography(
      [
        node({ key: 'a', latitude: 54.0, longitude: 13.0 }),
        node({ key: 'b', latitude: 54.02, longitude: 13.0 }),
      ],
      NOW,
    );

    expect(geo?.centreLatitude).toBeCloseTo(54.01, 4);
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

const geo = geography(
  [
    node({ key: 'a', latitude: 54.0, longitude: 13.0 }),
    node({ key: 'b', latitude: 54.02, longitude: 13.04 }),
  ],
  NOW,
)!;

describe('fit and onScreen', () => {
  it('brings everything inside the padding', () => {
    const view = fit(geo, 800, 600, 60);

    for (const placed of geo.placed) {
      const { x, y } = onScreen(placed.world, view, 800, 600);
      expect(x).toBeGreaterThanOrEqual(59);
      expect(x).toBeLessThanOrEqual(741);
      expect(y).toBeGreaterThanOrEqual(59);
      expect(y).toBeLessThanOrEqual(541);
    }
  });

  it('puts north up', () => {
    const view = fit(geo, 800, 600, 60);
    const northern = geo.placed.find((one) => one.node.key === 'b')!;
    const southern = geo.placed.find((one) => one.node.key === 'a')!;

    expect(onScreen(northern.world, view, 800, 600).y).toBeLessThan(
      onScreen(southern.world, view, 800, 600).y,
    );
  });

  it('goes as close as the nodes are, not to some fixed level', () => {
    // Two nodes a hundred metres apart deserve a hundred-metre view. An
    // earlier cap meant for the degenerate case applied to every fit and
    // left them huddled in the middle of an empty screen.
    const close = geography(
      [
        node({ key: 'a', latitude: 54.331026, longitude: 13.070254 }),
        node({ key: 'b', latitude: 54.33093, longitude: 13.06954 }),
      ],
      NOW,
    )!;

    expect(fit(close, 800, 600, 60).zoom).toBeGreaterThan(17);
  });

  it('looks closely rather than dividing by zero when everything is on one spot', () => {
    const stacked = geography(
      [
        node({ key: 'a', latitude: 54.0, longitude: 13.0 }),
        node({ key: 'b', latitude: 54.0, longitude: 13.0 }),
      ],
      NOW,
    )!;

    expect(fit(stacked, 800, 600, 60).zoom).toBe(16);
  });
});

describe('zoomAt', () => {
  it('keeps what is under the cursor under the cursor', () => {
    const view = fit(geo, 800, 600, 60);
    const one = geo.placed[0]!;
    const before = onScreen(one.world, view, 800, 600);

    const zoomed = zoomAt(view, 1.7, before.x, before.y, 800, 600);
    const after = onScreen(one.world, zoomed, 800, 600);

    expect(after.x).toBeCloseTo(before.x, 6);
    expect(after.y).toBeCloseTo(before.y, 6);
  });

  it('stops before a pixel means less than a reported coordinate does', () => {
    const view = { centre: { x: 0.5, y: 0.5 }, zoom: 21 };

    expect(zoomAt(view, 5, 400, 300, 800, 600).zoom).toBe(22);
    expect(zoomAt({ ...view, zoom: 1 }, -5, 400, 300, 800, 600).zoom).toBe(0);
  });
});

describe('panBy', () => {
  it('moves the map with the hand, not against it', () => {
    const view = fit(geo, 800, 600, 60);
    const one = geo.placed[0]!;
    const before = onScreen(one.world, view, 800, 600);

    const after = onScreen(one.world, panBy(view, 40, -25), 800, 600);

    expect(after.x).toBeCloseTo(before.x + 40, 6);
    expect(after.y).toBeCloseTo(before.y - 25, 6);
  });
});

describe('visibleTiles', () => {
  const view = { centre: toWorld(52.520008, 13.404954), zoom: 10 };

  it('covers the screen and no more than it has to', () => {
    const tiles = visibleTiles(view, 800, 600, 19);

    // The middle of the screen is the tile the centre falls in.
    expect(tiles.some((tile) => tile.x === 550 && tile.y === 335)).toBe(true);
    // 800×600 at 256-pixel tiles: at most 5×4 with the offsets at their worst.
    expect(tiles.length).toBeLessThanOrEqual(20);
    for (const tile of tiles) {
      expect(tile.left).toBeLessThan(800);
      expect(tile.top).toBeLessThan(600);
      expect(tile.left + tile.size).toBeGreaterThan(0);
      expect(tile.top + tile.size).toBeGreaterThan(0);
    }
  });

  it('scales the tiles instead of snapping the view to them', () => {
    // A fitted view lands on a fractional zoom. Snapping it would make the
    // map jump the moment it was drawn.
    const [tile] = visibleTiles({ ...view, zoom: 10.5 }, 800, 600, 19);

    expect(tile?.z).toBe(11);
    expect(tile?.size).toBeCloseTo(TILE_SIZE * 2 ** -0.5, 6);
  });

  it('never asks for a level the source does not have', () => {
    const tiles = visibleTiles({ ...view, zoom: 19.4 }, 800, 600, 17);

    expect(tiles.every((tile) => tile.z === 17)).toBe(true);
  });

  it('leaves the edge of the world empty rather than repeating it', () => {
    // Far enough west that half the screen is off the map.
    const edge = { centre: { x: 0.001, y: 0.5 }, zoom: 2 };
    const tiles = visibleTiles(edge, 800, 600, 19);

    expect(tiles.every((tile) => tile.x >= 0 && tile.y >= 0)).toBe(true);
    expect(tiles.every((tile) => tile.x < 4 && tile.y < 4)).toBe(true);
  });
});

describe('metresPerPixel', () => {
  it('shrinks with latitude, as Mercator does', () => {
    // The classic figure: about 156 km per pixel at zoom 0 on the equator.
    expect(metresPerPixel(0, 0)).toBeCloseTo(156_543, 0);
    expect(metresPerPixel(10, 0)).toBeCloseTo(152.87, 2);
    expect(metresPerPixel(10, 54.33)).toBeLessThan(metresPerPixel(10, 0));
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

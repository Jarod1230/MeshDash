/**
 * Turning what the mesh reports into something drawable.
 *
 * Kept apart from the drawing so it can be checked without a browser: every
 * function here is pure, and every one of them is the place where an honest
 * arrangement is decided rather than a pretty one.
 *
 * # Web-Mercator, because the tiles are
 *
 * Raster tiles exist in Web-Mercator and nowhere else. A surface that draws
 * both tiles and nodes has to speak the tiles' projection, or the two drift
 * apart the further from the centre one looks.
 *
 * Positions therefore become **world coordinates** first — the unit square
 * that Web-Mercator maps the world onto, `0…1` in each direction, north-west
 * at the origin — and only then pixels. The separation is what makes panning
 * and zooming cheap: a pan moves the centre, a zoom changes one number, and
 * neither touches the projection.
 *
 * The price of Mercator is that a scale bar is only true at one latitude. It
 * is computed for the middle of the screen and stated as such, which over the
 * span of a mesh — tens of kilometres — is a difference nobody can read off
 * the bar anyway.
 */

/** Edge length of a raster tile, in pixels. The whole raster world uses this. */
export const TILE_SIZE = 256;

/**
 * The latitude where Web-Mercator gives up.
 *
 * The projection stretches towards the poles without bound; every tile scheme
 * cuts it here so the world comes out square.
 */
const MERCATOR_LIMIT = 85.05112878;

/** Metres per pixel at the equator, at zoom 0. */
const EQUATOR_METRES_PER_PIXEL = 156_543.033_928_040_97;

/** Anything not heard for this long is drawn as faded, not as gone. */
export const STALE_SECONDS = 86_400;

/** Within this long, a node counts as currently answering. */
const FRESH_SECONDS = 3_600;

/**
 * How a node is doing, as far as anything was heard from it.
 *
 * Three states rather than two, because the middle one is the interesting
 * one: a repeater that was there an hour ago and is quiet now is neither
 * fine nor gone, and that is precisely the moment an operator wants to see.
 *
 * All three are statements about **being heard**, not about being up. A node
 * whose only path here broke is silent from here and busy elsewhere.
 */
export type Heard = 'jetzt' | 'still' | 'lange';

/** Which of the three a node is in. */
export function heard(lastSeen: number, now: number): Heard {
  const ago = (now - lastSeen) / 1000;

  if (ago < FRESH_SECONDS) return 'jetzt';
  if (ago < STALE_SECONDS) return 'still';

  return 'lange';
}

/** One node, as the ground surface needs it — whichever module it came from. */
export interface GroundNode {
  readonly key: string;
  readonly name: string;
  readonly latitude: number | null;
  readonly longitude: number | null;
  /** Hops to it, or null when no route is known. */
  readonly stations: number | null;
  /** When it was last heard, epoch milliseconds. */
  readonly lastSeen: number;
  /** Whether this is the node MeshDash is attached to. */
  readonly own: boolean;
  /** Where the position came from, for the ones that have one. */
  readonly source: 'advert' | 'telemetry' | null;
}

/** A point on the Web-Mercator unit square. */
export interface World {
  readonly x: number;
  readonly y: number;
}

/** A node placed on that square. */
export interface Placed {
  readonly node: GroundNode;
  readonly world: World;
  readonly stale: boolean;
}

/** What part of the world is on screen, and how closely. */
export interface View {
  /** The world coordinate in the middle of the screen. */
  readonly centre: World;
  /**
   * Zoom, as tile schemes count it: one step doubles the scale.
   *
   * Not an integer — a fitted view lands wherever it lands, and the tiles are
   * scaled to match rather than the view being snapped to them.
   */
  readonly zoom: number;
}

/** A geographic arrangement, or nothing when there is too little to arrange. */
export interface Geography {
  readonly placed: readonly Placed[];
  /** The latitude in the middle, which is where the scale bar is true. */
  readonly centreLatitude: number;
}

/**
 * Does this node say where it is?
 *
 * Zero is not a position: the firmware stores it for "unset", and treating it
 * as one would put half the mesh in the Gulf of Guinea.
 */
export function isPlaceable(node: GroundNode): boolean {
  return (
    node.latitude !== null &&
    node.longitude !== null &&
    !(node.latitude === 0 && node.longitude === 0)
  );
}

/** Projects a coordinate onto the Web-Mercator unit square. */
export function toWorld(latitude: number, longitude: number): World {
  const clamped = Math.min(Math.max(latitude, -MERCATOR_LIMIT), MERCATOR_LIMIT);
  const sine = Math.sin((clamped * Math.PI) / 180);

  return {
    x: (longitude + 180) / 360,
    y: 0.5 - Math.log((1 + sine) / (1 - sine)) / (4 * Math.PI),
  };
}

/** Back from the unit square to a latitude, for the scale bar. */
export function toLatitude(y: number): number {
  return (Math.atan(Math.sinh(Math.PI * (1 - 2 * y))) * 180) / Math.PI;
}

/** How many pixels one unit of world coordinate covers at this zoom. */
export function scaleOf(zoom: number): number {
  return TILE_SIZE * 2 ** zoom;
}

/**
 * Arranges every node that reports a position.
 *
 * Returns null below two placed nodes: one point is not a geography. It has no
 * extent, no scale and no neighbours — the topological arrangement says more
 * about a mesh like that than a lone dot in an empty field.
 */
export function geography(nodes: readonly GroundNode[], now: number): Geography | null {
  const placeable = nodes.filter(isPlaceable);
  if (placeable.length < 2) return null;

  const placed = placeable.map((node) => ({
    node,
    world: toWorld(node.latitude ?? 0, node.longitude ?? 0),
    stale: (now - node.lastSeen) / 1000 > STALE_SECONDS,
  }));

  const ys = placed.map((one) => one.world.y);

  return {
    placed,
    centreLatitude: toLatitude((Math.min(...ys) + Math.max(...ys)) / 2),
  };
}

/** How far apart the placed nodes are, on the unit square. */
function extent(geo: Geography): {
  readonly centre: World;
  readonly spanX: number;
  readonly spanY: number;
} {
  const xs = geo.placed.map((one) => one.world.x);
  const ys = geo.placed.map((one) => one.world.y);
  const minX = Math.min(...xs);
  const maxX = Math.max(...xs);
  const minY = Math.min(...ys);
  const maxY = Math.max(...ys);

  return {
    centre: { x: (minX + maxX) / 2, y: (minY + maxY) / 2 },
    spanX: maxX - minX,
    spanY: maxY - minY,
  };
}

/** How close to look when every node sits on the same spot. */
const CLOSE_ENOUGH = 16;

/**
 * How close a fitted view goes at most.
 *
 * Raster sources stop around here, and a metre per screen is finer than a mesh
 * is ever measured. Only a fit is capped — somebody who zooms in further is
 * asking for it and gets it.
 */
const FURTHEST_IN = 19;

/** A view that shows everything placed, with room to breathe. */
export function fit(geo: Geography, width: number, height: number, padding: number): View {
  const { centre, spanX, spanY } = extent(geo);
  const usableWidth = Math.max(width - 2 * padding, 1);
  const usableHeight = Math.max(height - 2 * padding, 1);

  // Nodes at one spot have no extent. Rather than dividing by zero, look as
  // closely as a reported position is worth.
  const forX = spanX > 0 ? Math.log2(usableWidth / (TILE_SIZE * spanX)) : Infinity;
  const forY = spanY > 0 ? Math.log2(usableHeight / (TILE_SIZE * spanY)) : Infinity;
  const tightest = Math.min(forX, forY);

  return {
    centre,
    zoom: clampZoom(Number.isFinite(tightest) ? Math.min(tightest, FURTHEST_IN) : CLOSE_ENOUGH),
  };
}

/** Where a world point lands on screen, in pixels from the top left. */
export function onScreen(
  world: World,
  view: View,
  width: number,
  height: number,
): { readonly x: number; readonly y: number } {
  const scale = scaleOf(view.zoom);

  return {
    x: width / 2 + (world.x - view.centre.x) * scale,
    y: height / 2 + (world.y - view.centre.y) * scale,
  };
}

/**
 * Zooms about a point on screen, so what is under the cursor stays under it.
 *
 * Zooming about the centre instead would drag whatever the reader is looking
 * at out from under them, which is exactly the thing that makes a map feel
 * broken.
 */
export function zoomAt(
  view: View,
  steps: number,
  x: number,
  y: number,
  width: number,
  height: number,
): View {
  const zoom = clampZoom(view.zoom + steps);
  const before = scaleOf(view.zoom);
  const after = scaleOf(zoom);

  // The world point under the cursor, held in place by moving the centre.
  const worldX = view.centre.x + (x - width / 2) / before;
  const worldY = view.centre.y + (y - height / 2) / before;

  return {
    centre: {
      x: worldX - (x - width / 2) / after,
      y: worldY - (y - height / 2) / after,
    },
    zoom,
  };
}

/**
 * How much one wheel event should change the zoom.
 *
 * A mouse wheel sends few large notches; a trackpad sends a stream of small
 * ones, dozens per gesture. Treating both as one fixed step means a mouse
 * behaves and a trackpad flies from the whole world to a single house in one
 * swipe — reported from a MacBook, and exactly what a fixed step does.
 *
 * So the step follows the size of the event, and is capped so no single flick
 * can cross more than half a zoom level.
 */
export function zoomStep(deltaY: number, deltaMode: number, pinch: boolean): number {
  // deltaMode: 0 pixels, 1 lines, 2 pages. Firefox reports lines.
  const pixels = deltaY * (deltaMode === 1 ? 16 : deltaMode === 2 ? 400 : 1);
  // A pinch arrives as a wheel event with the control key held — macOS and
  // Windows both do this — and its numbers are far smaller than a scroll's.
  const perPixel = pinch ? 0.012 : 0.004;

  return Math.min(Math.max(-pixels * perPixel, -0.6), 0.6);
}

/** Moves the view by a drag, in pixels. */
export function panBy(view: View, dx: number, dy: number): View {
  const scale = scaleOf(view.zoom);

  return {
    centre: { x: view.centre.x - dx / scale, y: view.centre.y - dy / scale },
    zoom: view.zoom,
  };
}

/**
 * Keeps the zoom in a range that means something.
 *
 * Below zero the whole world is smaller than one tile; past 22 a pixel is
 * finer than any position a node reports, which is six decimal places or about
 * ten centimetres.
 */
function clampZoom(zoom: number): number {
  return Math.min(Math.max(zoom, 0), 22);
}

/** How many metres one pixel covers, at the latitude it is asked about. */
export function metresPerPixel(zoom: number, latitude: number): number {
  return (EQUATOR_METRES_PER_PIXEL * Math.cos((latitude * Math.PI) / 180)) / 2 ** zoom;
}

/** One raster tile, and where it goes on screen. */
export interface TileAt {
  readonly z: number;
  readonly x: number;
  readonly y: number;
  readonly left: number;
  readonly top: number;
  /** Edge length on screen. Not 256 unless the zoom happens to be whole. */
  readonly size: number;
}

/**
 * Which tiles cover the screen, and where each one goes.
 *
 * The tile level is the view's zoom rounded to a whole number and capped at
 * what the source has; the tiles are then scaled to the actual zoom. Snapping
 * the view to whole zoom levels instead would make a fitted view jump the
 * moment it was drawn.
 *
 * Tiles outside the world are left out rather than wrapped. A mesh is a
 * region, and a copy of Europe at the edge of the screen would be a lie about
 * where the nodes are.
 */
export function visibleTiles(
  view: View,
  width: number,
  height: number,
  maxZoom: number,
): TileAt[] {
  const z = Math.min(Math.max(Math.round(view.zoom), 0), Math.floor(maxZoom));
  const scale = scaleOf(view.zoom);
  const count = 2 ** z;
  const size = scale / count;

  // The world coordinate at the top left corner of the screen.
  const originX = view.centre.x - width / 2 / scale;
  const originY = view.centre.y - height / 2 / scale;

  const firstX = Math.floor(originX * count);
  const lastX = Math.floor((originX + width / scale) * count);
  const firstY = Math.floor(originY * count);
  const lastY = Math.floor((originY + height / scale) * count);

  const tiles: TileAt[] = [];

  for (let y = Math.max(firstY, 0); y <= Math.min(lastY, count - 1); y += 1) {
    for (let x = Math.max(firstX, 0); x <= Math.min(lastX, count - 1); x += 1) {
      tiles.push({
        z,
        x,
        y,
        left: (x / count - originX) * scale,
        top: (y / count - originY) * scale,
        size,
      });
    }
  }

  return tiles;
}

/** A round number of metres that fits in about a quarter of the drawing. */
export function scaleStep(spanMetres: number): number {
  const target = spanMetres / 4;
  const magnitude = 10 ** Math.floor(Math.log10(target));
  const steps = [1, 2, 5, 10];

  // Largest round step that still fits, so the bar never overflows.
  let chosen = magnitude;
  for (const step of steps) {
    if (step * magnitude <= target) chosen = step * magnitude;
  }

  return chosen;
}

/** Metres as people say them. */
export function formatDistance(metres: number): string {
  return metres >= 1_000
    ? `${(metres / 1_000).toLocaleString('de-DE', { maximumFractionDigits: 1 })} km`
    : `${Math.round(metres)} m`;
}

/** A node placed on a ring, for the arrangement that has no geography. */
export interface Ringed {
  readonly node: GroundNode;
  /** Fraction of the available radius, 0 at the centre and 1 at the edge. */
  readonly radius: number;
  /** Radians. Means nothing — see the note in `rings`. */
  readonly angle: number;
  readonly hops: number;
  readonly stale: boolean;
}

/** How many hops out the outermost ring sits. */
const MAX_RING = 4;

/**
 * Arranges nodes by hop count when there is no geography to arrange them by.
 *
 * Distance from the centre means hops, which is a real measurement. The angle
 * means nothing at all, and that is the point: a force-directed graph would
 * look like a map and be one only by accident.
 *
 * A contact with no known route is not zero hops away. It goes on the
 * outermost ring, rather than being given a distance nobody measured.
 */
export function rings(nodes: readonly GroundNode[], now: number): Ringed[] {
  const byRing = new Map<number, GroundNode[]>();

  for (const node of nodes) {
    if (node.own) continue;
    const hops = node.stations === null ? MAX_RING : Math.min(node.stations + 1, MAX_RING);
    byRing.set(hops, [...(byRing.get(hops) ?? []), node]);
  }

  const placed: Ringed[] = [];

  for (const [hops, ring] of [...byRing.entries()].sort((a, b) => a[0] - b[0])) {
    ring.forEach((node, index) => {
      placed.push({
        node,
        radius: hops / MAX_RING,
        // Each ring starts at its own angle, so nodes do not line up in
        // spokes and suggest a relationship that is not there.
        angle: (index / ring.length) * Math.PI * 2 + hops * 0.7,
        hops,
        stale: (now - node.lastSeen) / 1000 > STALE_SECONDS,
      });
    });
  }

  return placed;
}

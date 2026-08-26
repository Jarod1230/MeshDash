/**
 * Turning what the mesh reports into something drawable.
 *
 * Kept apart from the drawing so it can be checked without a browser: every
 * function here is pure, and every one of them is the place where an honest
 * arrangement is decided rather than a pretty one.
 *
 * # Two spaces, not one
 *
 * Positions become **metres east and north of a centre** first, and only then
 * pixels. That separation is what makes panning and zooming cheap — a pan
 * moves the centre, a zoom changes the scale, and neither touches the
 * projection. It also keeps one scale for both axes, so a scale bar means the
 * same in every direction.
 *
 * # The projection is local, not Web-Mercator
 *
 * Longitudes are squeezed by the cosine of the mean latitude. Web-Mercator
 * stretches north-south against east-west, which would make a single scale bar
 * wrong in one direction. Over the span of a mesh — tens of kilometres — this
 * approximation is more accurate than any global projection.
 */

/** Metres per degree of latitude; the equatorial value is close enough here. */
const METRES_PER_DEGREE = 111_320;

/** Anything not heard for this long is drawn as faded, not as gone. */
export const STALE_SECONDS = 86_400;

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

/** A node placed in metre space, east and north of the centre. */
export interface Placed {
  readonly node: GroundNode;
  readonly east: number;
  readonly north: number;
  readonly stale: boolean;
}

/** What part of the world is on screen, and how closely. */
export interface View {
  /** Metres east of the centre that sits in the middle of the screen. */
  readonly east: number;
  /** Metres north of it. */
  readonly north: number;
  /** How many metres one pixel covers. Larger means further out. */
  readonly metresPerPixel: number;
}

/** Where the centre of the metre space lies in the real world. */
export interface Anchor {
  readonly latitude: number;
  readonly longitude: number;
}

/** A geographic arrangement, or nothing when there is too little to arrange. */
export interface Geography {
  readonly placed: readonly Placed[];
  readonly anchor: Anchor;
  /** Metres from edge to edge of what was placed, on the longer side. */
  readonly spanMetres: number;
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

/**
 * Arranges every node that reports a position, in metres from their centre.
 *
 * Returns null below two placed nodes: one point is not a geography. It has no
 * extent, no scale and no neighbours — the topological arrangement says more
 * about a mesh like that than a lone dot in an empty field.
 */
export function geography(nodes: readonly GroundNode[], now: number): Geography | null {
  const placeable = nodes.filter(isPlaceable);
  if (placeable.length < 2) return null;

  const latitudes = placeable.map((node) => node.latitude ?? 0);
  const longitudes = placeable.map((node) => node.longitude ?? 0);
  const centreLat = (Math.min(...latitudes) + Math.max(...latitudes)) / 2;
  const centreLon = (Math.min(...longitudes) + Math.max(...longitudes)) / 2;
  const lonScale = Math.cos((centreLat * Math.PI) / 180);

  const placed = placeable.map((node) => ({
    node,
    east: ((node.longitude ?? 0) - centreLon) * METRES_PER_DEGREE * lonScale,
    north: ((node.latitude ?? 0) - centreLat) * METRES_PER_DEGREE,
    stale: (now - node.lastSeen) / 1000 > STALE_SECONDS,
  }));

  const heightMetres = (Math.max(...latitudes) - Math.min(...latitudes)) * METRES_PER_DEGREE;
  const widthMetres =
    (Math.max(...longitudes) - Math.min(...longitudes)) * METRES_PER_DEGREE * lonScale;

  return {
    placed,
    anchor: { latitude: centreLat, longitude: centreLon },
    // Several nodes at one spot have no extent. A nominal kilometre keeps the
    // scale bar honest instead of dividing by zero.
    spanMetres: Math.max(heightMetres, widthMetres, 1_000),
  };
}

/** A view that shows everything placed, with room to breathe. */
export function fit(
  geo: Geography,
  width: number,
  height: number,
  padding: number,
): View {
  const easts = geo.placed.map((node) => node.east);
  const norths = geo.placed.map((node) => node.north);
  const spanEast = Math.max(Math.max(...easts) - Math.min(...easts), 1);
  const spanNorth = Math.max(Math.max(...norths) - Math.min(...norths), 1);

  const usableWidth = Math.max(width - 2 * padding, 1);
  const usableHeight = Math.max(height - 2 * padding, 1);

  return {
    east: (Math.min(...easts) + Math.max(...easts)) / 2,
    north: (Math.min(...norths) + Math.max(...norths)) / 2,
    // The looser of the two axes decides, so nothing falls outside.
    metresPerPixel: Math.max(spanEast / usableWidth, spanNorth / usableHeight, 0.1),
  };
}

/** Where a placed node lands on screen, in pixels from the top left. */
export function onScreen(
  placed: Placed,
  view: View,
  width: number,
  height: number,
): { readonly x: number; readonly y: number } {
  return {
    x: width / 2 + (placed.east - view.east) / view.metresPerPixel,
    // Screen y grows downwards, north does not.
    y: height / 2 - (placed.north - view.north) / view.metresPerPixel,
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
  factor: number,
  x: number,
  y: number,
  width: number,
  height: number,
): View {
  const next = clampScale(view.metresPerPixel * factor);
  const changed = next - view.metresPerPixel;

  return {
    east: view.east - (x - width / 2) * changed,
    north: view.north + (y - height / 2) * changed,
    metresPerPixel: next,
  };
}

/**
 * Keeps the scale in a range that still means something.
 *
 * Closer than a centimetre per pixel says more than any position is worth —
 * a reported coordinate is six decimal places, about ten centimetres. Further
 * out than a kilometre per pixel and the whole planet is a few screens wide.
 */
function clampScale(metresPerPixel: number): number {
  return Math.min(Math.max(metresPerPixel, 0.01), 1_000);
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

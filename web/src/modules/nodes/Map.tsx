import type { KnownContact } from './types';

/**
 * Where the nodes are, as far as they say so.
 *
 * # No tiles, on purpose
 *
 * A LoRa mesh exists where infrastructure does not, and MeshDash often runs on
 * a box without an uplink. Tiles from a public server would leave a grey
 * rectangle exactly there — and everywhere else they would tell that server
 * where the mesh stands, on every glance at the map. See ADR-0010. What the
 * map does show is the thing an operator actually reads off it: how far apart
 * things are, and in which direction.
 *
 * # The projection is local, not Web-Mercator
 *
 * Longitudes are squeezed by the cosine of the mean latitude. Web-Mercator
 * stretches north-south against east-west, which would make a single scale bar
 * wrong in one direction. Over the span of a mesh — tens of kilometres — this
 * approximation is more accurate than any global projection.
 */
const SIZE = 480;
const PADDING = 34;
/** Metres per degree of latitude; the equatorial value is close enough here. */
const METRES_PER_DEGREE = 111_320;

/** A node with a position, ready to draw. */
export interface Placed {
  readonly contact: KnownContact;
  readonly x: number;
  readonly y: number;
  readonly stale: boolean;
}

/** What the map needs to draw itself. */
export interface Projection {
  readonly placed: readonly Placed[];
  /** Metres covered by the whole drawing, on the longer side. */
  readonly spanMetres: number;
  /** Pixels per metre, for the scale bar. */
  readonly pixelsPerMetre: number;
}

const DAY_SECONDS = 86_400;

/** Contacts that report where they are. */
export function positioned(contacts: readonly KnownContact[]): KnownContact[] {
  // Zero is not a position: the firmware stores it for "unset", and treating
  // it as one would put half the mesh in the Gulf of Guinea.
  return contacts.filter(
    (contact) =>
      contact.latitude !== null &&
      contact.longitude !== null &&
      !(contact.latitude === 0 && contact.longitude === 0),
  );
}

/** Projects contacts into the drawing, keeping distances comparable. */
export function project(contacts: readonly KnownContact[], now: number): Projection | null {
  const withPosition = positioned(contacts);
  if (withPosition.length === 0) return null;

  const latitudes = withPosition.map((contact) => contact.latitude ?? 0);
  const longitudes = withPosition.map((contact) => contact.longitude ?? 0);
  const minLat = Math.min(...latitudes);
  const maxLat = Math.max(...latitudes);
  const minLon = Math.min(...longitudes);
  const maxLon = Math.max(...longitudes);

  const meanLat = (minLat + maxLat) / 2;
  const lonScale = Math.cos((meanLat * Math.PI) / 180);

  // Metres across, so both axes share one scale and the bar means something.
  const heightMetres = (maxLat - minLat) * METRES_PER_DEGREE;
  const widthMetres = (maxLon - minLon) * METRES_PER_DEGREE * lonScale;

  // A single node, or several at one spot, has no extent. Give it a nominal
  // one kilometre so the scale bar stays honest instead of dividing by zero.
  const span = Math.max(heightMetres, widthMetres, 1_000);
  const usable = SIZE - 2 * PADDING;
  const pixelsPerMetre = usable / span;

  const centreLat = (minLat + maxLat) / 2;
  const centreLon = (minLon + maxLon) / 2;

  const placed = withPosition.map((contact) => {
    const north = ((contact.latitude ?? 0) - centreLat) * METRES_PER_DEGREE;
    const east = ((contact.longitude ?? 0) - centreLon) * METRES_PER_DEGREE * lonScale;
    const age = (now - new Date(contact.last_seen).getTime()) / 1000;

    return {
      contact,
      x: SIZE / 2 + east * pixelsPerMetre,
      // Screen y grows downwards, north does not.
      y: SIZE / 2 - north * pixelsPerMetre,
      stale: age > DAY_SECONDS,
    };
  });

  return { placed, spanMetres: span, pixelsPerMetre };
}

/** A round number of metres that fits in about a quarter of the drawing. */
export function scaleStep(spanMetres: number): number {
  const target = spanMetres / 4;
  const magnitude = 10 ** Math.floor(Math.log10(target));
  const steps = [1, 2, 5, 10];

  // Largest round step that still fits, so the bar never overflows the map.
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

export function Map({
  contacts,
  now,
}: {
  readonly contacts: readonly KnownContact[];
  readonly now: number;
}) {
  const projection = project(contacts, now);

  if (projection === null) {
    return (
      <p className="px-4 py-6 text-sm text-mesh-muted">
        Kein Knoten meldet eine Position. Ein Node sendet sie nur mit, wenn sie bei ihm eingestellt
        ist — die meisten tun das nicht.
      </p>
    );
  }

  const { placed, spanMetres, pixelsPerMetre } = projection;
  const step = scaleStep(spanMetres);
  const barWidth = step * pixelsPerMetre;
  const withoutPosition = contacts.length - placed.length;

  return (
    <figure className="p-4">
      <svg
        viewBox={`0 0 ${SIZE} ${SIZE}`}
        className="mx-auto block h-auto w-full max-w-lg"
        role="img"
        aria-label={`Karte mit ${placed.length} verorteten Knoten über etwa ${formatDistance(spanMetres)}`}
      >
        {placed.map((node) => (
          <g key={node.contact.public_key} opacity={node.stale ? 0.45 : 1}>
            <circle
              cx={node.x}
              cy={node.y}
              r={5}
              className={node.stale ? 'fill-mesh-border' : 'fill-mesh-accent'}
            />
            <text
              x={node.x}
              y={node.y - 10}
              textAnchor="middle"
              fontSize={10}
              className={node.stale ? 'fill-mesh-faint' : 'fill-mesh-muted'}
            >
              {node.contact.name}
            </text>
            <title>
              {node.contact.name} · {node.contact.latitude?.toFixed(5)},{' '}
              {node.contact.longitude?.toFixed(5)}
            </title>
          </g>
        ))}

        <g transform={`translate(${PADDING}, ${SIZE - 18})`}>
          <line x1={0} y1={0} x2={barWidth} y2={0} className="stroke-mesh-muted" strokeWidth={2} />
          <line x1={0} y1={-4} x2={0} y2={4} className="stroke-mesh-muted" strokeWidth={2} />
          <line
            x1={barWidth}
            y1={-4}
            x2={barWidth}
            y2={4}
            className="stroke-mesh-muted"
            strokeWidth={2}
          />
          <text x={barWidth / 2} y={-8} textAnchor="middle" fontSize={10} className="fill-mesh-muted">
            {formatDistance(step)}
          </text>
        </g>
      </svg>

      <figcaption className="mt-3 space-y-2 text-xs text-mesh-faint">
        <p>
          Norden ist oben. Blass heißt: seit über einem Tag nicht gehört.
          {withoutPosition > 0 && (
            <>
              {' '}
              {withoutPosition} {withoutPosition === 1 ? 'Knoten meldet' : 'Knoten melden'} keine
              Position und {withoutPosition === 1 ? 'fehlt' : 'fehlen'} hier.
            </>
          )}
        </p>
        <ul className="flex flex-wrap gap-x-3 gap-y-1">
          {placed.map((node) => (
            <li key={node.contact.public_key}>
              <a
                href={`https://www.openstreetmap.org/?mlat=${node.contact.latitude}&mlon=${node.contact.longitude}#map=13/${node.contact.latitude}/${node.contact.longitude}`}
                target="_blank"
                rel="noreferrer noopener"
                className="text-mesh-accent underline-offset-2 hover:underline focus-visible:outline focus-visible:outline-2 focus-visible:outline-mesh-accent"
              >
                {node.contact.name} in OpenStreetMap
              </a>
            </li>
          ))}
        </ul>
      </figcaption>
    </figure>
  );
}

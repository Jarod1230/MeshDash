import { hopCount, type KnownContact } from './types';

/**
 * Who hears whom, drawn from what the node actually knows.
 *
 * # Why this is a ring chart and not a map
 *
 * A force-directed graph would look like a map and be one only by accident:
 * almost no node reports coordinates, so any position would be the layout
 * algorithm's invention. What *is* known is how far away something is in hops
 * — and that is a real measurement. So distance from the centre means hops,
 * and the angle means nothing at all, which is the honest arrangement.
 *
 * Nodes not heard in a day fade out rather than vanishing: "was here, is not
 * answering" is the very thing an operator is looking for.
 */
const SIZE = 340;
const CENTRE = SIZE / 2;
const MAX_RING = 4;
/**
 * The innermost ring keeps its distance from the centre: a direct neighbour
 * placed too close had its label sitting on top of "eigener Node". The
 * outermost stays inside the canvas, where a wider step ran the line off the
 * edge into nothing.
 */
const FIRST_RING = 52;
const LAST_RING = CENTRE - 32;
const RING_STEP = (LAST_RING - FIRST_RING) / (MAX_RING - 1);
const DAY_SECONDS = 86_400;
/** Above this many nodes the labels overlap and are dropped. */
const LABEL_LIMIT = 9;

interface Placed {
  readonly contact: KnownContact;
  readonly x: number;
  readonly y: number;
  readonly hops: number;
  readonly stale: boolean;
}

/** Places contacts on rings by hop count, spread evenly around each ring. */
export function place(contacts: readonly KnownContact[], now: number): Placed[] {
  const byRing = new Map<number, KnownContact[]>();

  for (const contact of contacts) {
    // A direct neighbour has no hops in between; it sits on the first ring.
    const hops = Math.min(hopCount(contact.path) + 1, MAX_RING);
    const ring = byRing.get(hops) ?? [];
    ring.push(contact);
    byRing.set(hops, ring);
  }

  const placed: Placed[] = [];

  for (const [hops, ring] of [...byRing.entries()].sort((a, b) => a[0] - b[0])) {
    const radius = FIRST_RING + (hops - 1) * RING_STEP;
    ring.forEach((contact, index) => {
      // Start each ring at a different angle so nodes do not line up in
      // spokes, which would suggest a relationship that is not there.
      const angle = (index / ring.length) * Math.PI * 2 + hops * 0.7;
      const age = (now - new Date(contact.last_seen).getTime()) / 1000;
      placed.push({
        contact,
        x: CENTRE + Math.cos(angle) * radius,
        y: CENTRE + Math.sin(angle) * radius,
        hops,
        stale: age > DAY_SECONDS,
      });
    });
  }

  return placed;
}

export function Topology({
  contacts,
  now,
}: {
  readonly contacts: readonly KnownContact[];
  readonly now: number;
}) {
  const placed = place(contacts, now);
  const rings = [...new Set(placed.map((node) => node.hops))].sort((a, b) => a - b);

  if (placed.length === 0) {
    return (
      <p className="px-4 py-6 text-sm text-mesh-muted">
        Noch keine Knoten bekannt. Sobald der Node Kontakte meldet oder ein Advert eintrifft,
        erscheinen sie hier.
      </p>
    );
  }

  return (
    <figure className="p-4">
      <svg
        viewBox={`0 0 ${SIZE} ${SIZE}`}
        className="mx-auto block h-auto w-full max-w-md"
        role="img"
        aria-label={`Netzansicht mit ${placed.length} Knoten, angeordnet nach Anzahl der Zwischenstationen`}
      >
        {rings.map((hops) => (
          <circle
            key={hops}
            cx={CENTRE}
            cy={CENTRE}
            r={FIRST_RING + (hops - 1) * RING_STEP}
            fill="none"
            className="stroke-mesh-border"
            strokeWidth={1}
          />
        ))}

        {placed.map((node) => (
          <line
            key={`line-${node.contact.public_key}`}
            x1={CENTRE}
            y1={CENTRE}
            x2={node.x}
            y2={node.y}
            className={node.stale ? 'stroke-mesh-border' : 'stroke-mesh-accent-dim'}
            strokeWidth={1}
            strokeDasharray={node.hops > 1 ? '3 3' : undefined}
          />
        ))}

        {/* No label under the centre: a direct neighbour sits close enough
            that the two collided. The filled dot plus the caption carry it. */}
        <circle cx={CENTRE} cy={CENTRE} r={9} className="fill-mesh-accent">
          <title>Der eigene Node</title>
        </circle>

        {placed.map((node) => (
          <g key={node.contact.public_key} opacity={node.stale ? 0.45 : 1}>
            <circle
              cx={node.x}
              cy={node.y}
              r={6}
              className={node.stale ? 'fill-mesh-border' : 'fill-mesh-surface stroke-mesh-accent'}
              strokeWidth={1.5}
            />
            {placed.length <= LABEL_LIMIT && (
              <text
                x={node.x}
                y={node.y - 11}
                textAnchor="middle"
                fontSize={10}
                className={node.stale ? 'fill-mesh-faint' : 'fill-mesh-muted'}
              >
                {node.contact.name}
              </text>
            )}
            <title>
              {node.contact.name} · {node.hops === 1 ? 'direkt' : `${node.hops - 1} Zwischenstationen`}
            </title>
          </g>
        ))}
      </svg>

      <figcaption className="mt-3 text-xs text-mesh-faint">
        In der Mitte steht der eigene Node. Der Abstand heißt Zwischenstationen, nicht Entfernung;
        die Richtung bedeutet nichts — Koordinaten meldet kaum ein Knoten, und eine erfundene
        Anordnung sähe aus wie eine Karte, ohne eine zu sein. Blass heißt: seit über einem Tag
        nicht gehört.
      </figcaption>
    </figure>
  );
}

import { useMemo, useRef, useState } from 'react';
import { useSearchParams } from 'react-router-dom';
import { useLiveReload, type AppEvent } from '../lib/events';
import { isAdvert, isReceivedPacket } from '../lib/pushes';
import { useNow } from '../lib/useNow';
import { useResource } from '../lib/useResource';
import type { KnownContact, Trace } from '../modules/nodes/types';
import { links, strokeFor, type HeardBy, type MeshLink } from './links';
import { NodePanel } from './NodePanel';
import { useHeardRate } from './useHeardRate';
import { useSize } from './useSize';
import { useTiles, tileKey } from './useTiles';
import {
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
  visibleTiles,
  zoomAt,
  type Geography as GeographyOf,
  type GroundNode,
  type Heard,
  type Placed,
  type View,
} from './projection';

/**
 * The surface everything else lies on.
 *
 * MeshDash opens here and never leaves: the pages are a shutter over this
 * drawing, not a place it navigates to. That is what keeps the section of the
 * map an operator is looking at from being rebuilt every time they read a
 * message — see ADR-0011.
 *
 * # It draws what the data supports, not what a map is supposed to look like
 *
 * With two or more reported positions this is a geography. Below that it is
 * the ring arrangement: distance from the centre means hops, which is measured,
 * and the angle means nothing, which is honest. Either way it says how many
 * nodes it cannot place, rather than quietly leaving them out.
 *
 * # The base map is optional and off by default
 *
 * Tiles come from MeshDash when the operator has named a source, and from
 * nowhere when they have not. Without them the geography is drawn on an empty
 * ground, which is what MeshDash shipped with and what it falls back to where
 * there is no uplink.
 */
const PADDING = 90;
/** Below this many pixels apart, two labels are on top of each other. */
const LABEL_SPACE = 64;

interface SelfNode {
  readonly public_key: string;
  readonly name: string;
  readonly latitude: number | null;
  readonly longitude: number | null;
}

/** What `/api/v1/tiles` answers. */
interface TileInfo {
  readonly available: boolean;
  readonly attribution: string;
  readonly max_zoom: number;
}

export function Ground() {
  const now = useNow();
  const [attach, size] = useSize();
  const contacts = useResource<KnownContact[]>('/nodes/contacts');
  const status = useResource<{ node_self: SelfNode | null }>('/system/status');
  const tiles = useResource<TileInfo>('/tiles');
  // Every trace ever measured, because each one is a statement about a leg
  // that stays true until something contradicts it.
  const traces = useResource<Trace[]>('/nodes/traces?limit=200');
  // What the node overheard: who forwarded to whom, accumulated from packets
  // nobody had to send for us. Bounded by the number of prefixes, so it can be
  // asked for whole.
  const overheard = useResource<HeardBy[]>('/traffic/links');

  useLiveReload(
    (event: AppEvent) => event.type === 'push' && isAdvert(event.payload),
    () => contacts.reload(),
  );
  // A heard packet can name a pair nobody had seen before, so the summary is
  // asked for again. Cheap: it is a handful of rows, not the packet log.
  useLiveReload(
    (event: AppEvent) => event.type === 'push' && isReceivedPacket(event.payload),
    () => overheard.reload(),
  );

  const nodes = useMemo(
    () => assemble(contacts.data ?? [], status.data?.node_self ?? null),
    [contacts.data, status.data],
  );
  const geo = useMemo(() => geography(nodes, now), [nodes, now]);
  const mesh = useMemo(
    () => links(nodes, traces.data ?? [], overheard.data ?? [], now),
    [nodes, traces.data, overheard.data, now],
  );

  // The selection lives in the address, so a link to a dot opens the same
  // thing a click on it does. A path would say "another view"; this is the
  // same view with something picked out — see ADR-0014.
  const [params, setParams] = useSearchParams();
  const chosen = params.get('knoten');
  // Layers refine this view rather than being another view, so they ride in
  // the query string — see ADR-0014. Only the deviation is written down; an
  // address without it means the layer is on.
  const showLinks = params.get('verbindungen') !== 'aus';
  const selected = nodes.find((node) => node.key === chosen) ?? null;

  const select = (key: string | null) => {
    const next = new URLSearchParams(params);
    if (key === null) next.delete('knoten');
    else next.set('knoten', key);
    setParams(next, { replace: true });
  };

  const toggleLinks = () => {
    const next = new URLSearchParams(params);
    if (showLinks) next.set('verbindungen', 'aus');
    else next.delete('verbindungen');
    setParams(next, { replace: true });
  };

  return (
    <div ref={attach} className="absolute inset-0 overflow-hidden bg-mesh-bg">
      {size.width > 0 &&
        (geo === null ? (
          <Rings nodes={nodes} now={now} size={size} selected={chosen} onSelect={select} />
        ) : (
          <Geography
            geo={geo}
            nodes={nodes}
            now={now}
            size={size}
            tiles={tiles.data ?? null}
            selected={chosen}
            onSelect={select}
            mesh={showLinks ? mesh : []}
            linksOn={showLinks}
            onToggleLinks={toggleLinks}
          />
        ))}

      {selected !== null && (
        <NodePanel node={selected} now={now} onClose={() => select(null)} />
      )}
    </div>
  );
}

/** Brings the contact list and this node together into one set to draw. */
function assemble(contacts: readonly KnownContact[], own: SelfNode | null): GroundNode[] {
  const nodes: GroundNode[] = contacts.map((contact) => ({
    key: contact.public_key,
    name: contact.name,
    latitude: contact.latitude,
    longitude: contact.longitude,
    stations: contact.stations,
    lastSeen: new Date(contact.last_seen).getTime(),
    own: false,
    source: contact.position_source,
  }));

  if (own === null) return nodes;

  // This node is not a contact of itself, so it is never in the list — and it
  // is the one position that is certain. Drawing the mesh without its own
  // centre would leave out the only node whose place is not in question.
  return [
    ...nodes.filter((node) => node.key !== own.public_key),
    {
      key: own.public_key,
      name: own.name,
      latitude: own.latitude,
      longitude: own.longitude,
      stations: 0,
      lastSeen: Date.now(),
      own: true,
      source: 'advert',
    },
  ];
}

function Geography({
  geo,
  nodes,
  now,
  size,
  tiles,
  selected,
  onSelect,
  mesh,
  linksOn,
  onToggleLinks,
}: {
  readonly geo: GeographyOf;
  readonly nodes: readonly GroundNode[];
  readonly now: number;
  readonly size: { readonly width: number; readonly height: number };
  readonly tiles: TileInfo | null;
  readonly selected: string | null;
  readonly onSelect: (key: string | null) => void;
  readonly mesh: readonly MeshLink[];
  readonly linksOn: boolean;
  readonly onToggleLinks: () => void;
}) {
  // Null means "whatever fits". Once the reader has moved, their view is kept
  // as it is — including across a resize, which is what a map does.
  const [moved, setMoved] = useState<View | null>(null);
  // Not pointer capture: capturing makes the SVG the target of the following
  // click, so a click on a node would never reach the node. Instead the drag
  // is tracked here, and a click that came out of a drag is ignored — nobody
  // means to open a node by letting go of the map on top of it.
  const drag = useRef({ active: false, x: 0, y: 0, moved: false });

  const view = moved ?? fit(geo, size.width, size.height, PADDING);
  const open = (key: string) => {
    if (drag.current.moved) return;
    onSelect(key === selected ? null : key);
  };

  const covering = tiles?.available === true ? visibleTiles(view, size.width, size.height, tiles.max_zoom) : [];
  const images = useTiles(covering, tiles?.available === true);

  const placed = geo.placed.map((one) => ({
    one,
    at: onScreen(one.world, view, size.width, size.height),
  }));
  const labelled = declutter(placed);
  const where = new Map(placed.map(({ one, at }) => [one.node.key, at]));
  // A link with an unplaced end cannot be drawn anywhere honest, so it is not
  // drawn at all — and the note below says how many that was.
  const drawable = mesh.filter((link) => where.has(link.from) && where.has(link.to));

  // True at the middle of the screen and nowhere else — that is Mercator, and
  // over the span of a mesh the difference is smaller than the bar is wide.
  const centreLatitude = toLatitude(view.centre.y);
  const perPixel = metresPerPixel(view.zoom, centreLatitude);
  const step = scaleStep(size.width * perPixel);
  const missing = nodes.filter((node) => !isPlaceable(node)).length;

  return (
    <>
      <svg
        width={size.width}
        height={size.height}
        className="block cursor-grab touch-none select-none active:cursor-grabbing"
        role="img"
        aria-label={`Karte mit ${geo.placed.length} verorteten Knoten`}
        onWheel={(event) => {
          const box = event.currentTarget.getBoundingClientRect();
          setMoved(
            zoomAt(
              view,
              event.deltaY > 0 ? -0.4 : 0.4,
              event.clientX - box.left,
              event.clientY - box.top,
              size.width,
              size.height,
            ),
          );
        }}
        onPointerDown={(event) => {
          drag.current = { active: true, x: event.clientX, y: event.clientY, moved: false };
        }}
        onPointerMove={(event) => {
          const from = drag.current;
          if (!from.active) return;

          const dx = event.clientX - from.x;
          const dy = event.clientY - from.y;
          // A few pixels are a hand not holding still, not a pan.
          if (Math.abs(dx) > 3 || Math.abs(dy) > 3) from.moved = true;

          drag.current = { ...from, x: event.clientX, y: event.clientY };
          setMoved((previous) => panBy(previous ?? view, dx, dy));
        }}
        onPointerUp={() => {
          // `moved` deliberately survives: the click arrives after this.
          drag.current.active = false;
        }}
        onPointerLeave={() => {
          drag.current.active = false;
        }}
      >
        {covering.map((tile) => {
          const image = images.get(tileKey(tile));
          if (image === undefined || image === '') return null;

          return (
            <image
              key={tileKey(tile)}
              href={image}
              x={tile.left}
              y={tile.top}
              width={tile.size}
              height={tile.size}
              // Neighbouring tiles are drawn edge to edge; a fractional pixel
              // between them shows as a seam across the whole map.
              style={{ imageRendering: 'auto' }}
            />
          );
        })}

        {drawable.map((link) => {
          const from = where.get(link.from);
          const to = where.get(link.to);
          if (from === undefined || to === undefined) return null;

          return (
            <line
              key={link.id}
              x1={from.x}
              y1={from.y}
              x2={to.x}
              y2={to.y}
              className="stroke-mesh-accent"
              strokeWidth={strokeFor(link.snr)}
              strokeLinecap="round"
              opacity={link.snr === null ? 0.4 : 0.75}
            >
              <title>
                {describeLink(link)}
              </title>
            </line>
          );
        })}

        {placed.map(({ one, at }, index) => (
          <Node
            key={one.node.key}
            placed={one}
            x={at.x}
            y={at.y}
            state={heard(one.node.lastSeen, now)}
            label={labelled.has(index) || one.node.key === selected}
            chosen={one.node.key === selected}
            onOpen={() => open(one.node.key)}
          />
        ))}
      </svg>

      {/* The layer switch sits above the scale, in the corner ADR-0011 gives
          it. One switch today; the traffic layer joins it. */}
      <div className="absolute bottom-12 left-4 flex items-center gap-2">
        <HeardRate />
        <button
          type="button"
          onClick={onToggleLinks}
          aria-pressed={linksOn}
          className={`rounded-md border px-2.5 py-1 text-xs backdrop-blur focus-visible:outline focus-visible:outline-2 focus-visible:outline-mesh-accent ${
            linksOn
              ? 'border-mesh-accent bg-mesh-surface/90 text-mesh-text'
              : 'border-mesh-border bg-mesh-surface/70 text-mesh-muted hover:text-mesh-text'
          }`}
        >
          Verbindungen
        </button>
      </div>

      {/* Scale and credit share the bottom left, in that order. Kept in one
          row because two absolutely positioned boxes near the same corner
          find each other sooner or later — the credit sat on the scale bar's
          number and hid it. */}
      <div className="pointer-events-none absolute bottom-4 left-4 flex flex-wrap items-center gap-x-3 gap-y-1">
        <ScaleBar step={step} pixels={step / perPixel} />
        {tiles?.available === true && tiles.attribution !== '' && (
          <span className="rounded bg-mesh-surface/80 px-1.5 py-0.5 text-[11px] text-mesh-faint backdrop-blur">
            {tiles.attribution}
          </span>
        )}
      </div>

      {/* Each line carries its own background rather than the block having
          one: over a light basemap, faint text on nothing is unreadable, and
          a single box around them all would be a grey slab across the map. */}
      <div className="pointer-events-none absolute right-4 bottom-14 flex max-w-[min(30rem,calc(100%-9rem))] flex-col items-end gap-1 text-right text-xs text-mesh-faint [&>span]:rounded [&>span]:bg-mesh-surface/80 [&>span]:px-1.5 [&>span]:py-0.5 [&>span]:backdrop-blur">
        <span>
          Norden ist oben · <Dot className="fill-mesh-accent" /> in der letzten Stunde gehört ·{' '}
          <Dot className="fill-mesh-muted" /> heute · <Dot className="fill-none" hollow /> länger
          nicht
        </span>
        {missing > 0 && (
          <span>
            {missing} {missing === 1 ? 'Knoten meldet' : 'Knoten melden'} keine Position und{' '}
            {missing === 1 ? 'fehlt' : 'fehlen'} hier.
          </span>
        )}
        {labelled.size < placed.length && (
          <span>
            {placed.length - labelled.size}{' '}
            {placed.length - labelled.size === 1 ? 'Name liegt' : 'Namen liegen'} zu dicht
            beieinander und {placed.length - labelled.size === 1 ? 'steht' : 'stehen'} nur im
            Tooltip.
          </span>
        )}
        {linksOn && drawable.length > 0 && (
          <span>Linie heißt: dieser Weg wurde beobachtet · dicker heißt besser gehört</span>
        )}
        {linksOn && mesh.length === 0 && (
          // An empty layer without a reason reads as "there are no
          // connections", which would be a claim about the mesh. The claim
          // here is about what has been observed, and that is a different
          // sentence.
          <span>
            Noch kein Weg belegt. Ein Weg entsteht, sobald der Node eine Route zu einem Kontakt
            kennt oder ein „Weg messen" ihn abläuft.
          </span>
        )}
        {linksOn && mesh.length > drawable.length && (
          <span>
            {mesh.length - drawable.length}{' '}
            {mesh.length - drawable.length === 1 ? 'Verbindung führt' : 'Verbindungen führen'} zu
            einem Knoten ohne Position und {mesh.length - drawable.length === 1 ? 'fehlt' : 'fehlen'}{' '}
            hier.
          </span>
        )}
        {tiles !== null && !tiles.available && (
          <span>
            Ohne Kartenquelle. Eine lässt sich unter <span className="tabular">[modules.tiles]</span>{' '}
            eintragen.
          </span>
        )}
      </div>

      {moved !== null && (
        <button
          type="button"
          onClick={() => setMoved(null)}
          className="absolute right-4 bottom-4 rounded-md border border-mesh-border bg-mesh-surface/90 px-2.5 py-1 text-xs text-mesh-muted backdrop-blur hover:text-mesh-text focus-visible:outline focus-visible:outline-2 focus-visible:outline-mesh-accent"
        >
          alles zeigen
        </button>
      )}
    </>
  );
}

/**
 * Whether anything is on the air right now.
 *
 * A quiet mesh and a dead connection look the same without this. Says nothing
 * at all until a packet has been heard — a confident "0" before the first
 * event would be a claim, not a measurement.
 */
function HeardRate() {
  const rate = useHeardRate();
  if (rate === 0) return null;

  return (
    <span
      className="rounded-md border border-mesh-border bg-mesh-surface/90 px-2.5 py-1 text-xs text-mesh-muted backdrop-blur"
      title="Pakete, die dieser Node in der letzten Minute gehört hat — auch fremde"
    >
      <span className="tabular text-mesh-accent">{rate}</span> Pakete/Min
    </span>
  );
}

/**
 * What a line is, in one sentence.
 *
 * Each of the three sources says something different, and saying "measured"
 * about all of them would be the one wrong word: only a trace measures.
 */
function describeLink(link: MeshLink): string {
  const what =
    link.kind === 'direkt'
      ? 'direkt erreichbar'
      : link.kind === 'verfolgt'
        ? 'gemessener Weg'
        : `mitgehört${link.heard === null ? '' : ` in ${link.heard} ${link.heard === 1 ? 'Paket' : 'Paketen'}`}`;

  return `${what}${link.snr === null ? ' · Güte nicht gemessen' : ` · ${link.snr.toFixed(1)} dB`}`;
}

/**
 * Which labels can be drawn without landing on each other.
 *
 * Found by looking at a real mesh: two nodes fifty metres apart had their
 * names stacked and neither was readable. Dropping the later one keeps the
 * drawing legible — and the surface says how many it dropped, so a missing
 * name is not mistaken for a missing node.
 */
function declutter(
  placed: readonly { readonly at: { readonly x: number; readonly y: number } }[],
): Set<number> {
  const kept: { x: number; y: number }[] = [];
  const indices = new Set<number>();

  placed.forEach(({ at }, index) => {
    const collides = kept.some(
      (other) => Math.abs(other.x - at.x) < LABEL_SPACE && Math.abs(other.y - at.y) < 16,
    );
    if (collides) return;
    kept.push(at);
    indices.add(index);
  });

  return indices;
}

/**
 * How each state is drawn.
 *
 * Filled means answering, hollow means it stopped — a shape rather than only
 * a shade, because a shade is what a light basemap eats first, and because
 * "was here, is not answering" is the finding an operator is after.
 */
const LOOKS: Record<Heard, { readonly fill: string; readonly label: string }> = {
  jetzt: { fill: 'fill-mesh-accent', label: 'fill-mesh-muted' },
  still: { fill: 'fill-mesh-muted', label: 'fill-mesh-muted' },
  lange: { fill: 'fill-none', label: 'fill-mesh-faint' },
};

function Node({
  placed,
  x,
  y,
  state,
  label,
  chosen,
  onOpen,
}: {
  readonly placed: Placed;
  readonly x: number;
  readonly y: number;
  readonly state: Heard;
  readonly label: boolean;
  readonly chosen: boolean;
  readonly onOpen: () => void;
}) {
  const { node } = placed;
  const look = LOOKS[state];

  return (
    <g
      role="button"
      tabIndex={0}
      className="cursor-pointer focus-visible:outline focus-visible:outline-2 focus-visible:outline-mesh-accent"
      onClick={onOpen}
      onKeyDown={(event) => {
        if (event.key === 'Enter' || event.key === ' ') onOpen();
      }}
    >
      {chosen && (
        <circle cx={x} cy={y} r={13} className="fill-none stroke-mesh-accent" strokeWidth={2} />
      )}
      {node.own && (
        <circle
          cx={x}
          cy={y}
          r={10}
          className="fill-none stroke-mesh-accent"
          strokeWidth={1}
          strokeDasharray="2 3"
        />
      )}
      <circle
        cx={x}
        cy={y}
        r={node.own ? 6 : 5}
        className={look.fill}
        // A dot on a photographic base needs an edge, or it disappears into
        // whatever it happens to sit on. For the hollow state the edge is the
        // dot.
        stroke={state === 'lange' ? 'currentColor' : 'rgba(0,0,0,0.55)'}
        strokeWidth={state === 'lange' ? 2 : 1}
      />
      {label && (
        <text
          x={x}
          y={y - 12}
          textAnchor="middle"
          fontSize={11}
          className={look.label}
          // Same reason as the dot: a name has to stay readable over a map.
          paintOrder="stroke"
          stroke="rgba(0,0,0,0.55)"
          strokeWidth={2.5}
        >
          {node.name}
        </text>
      )}
      <title>
        {node.name}
        {node.own ? ' · dieser Node' : ''} · {node.latitude?.toFixed(5)},{' '}
        {node.longitude?.toFixed(5)}
        {node.source === 'telemetry' ? ' · Position aus Telemetrie' : ''}
      </title>
    </g>
  );
}

/** The legend's little circle, drawn the same way the map draws them. */
function Dot({ className, hollow = false }: { readonly className: string; readonly hollow?: boolean }) {
  return (
    <svg width={9} height={9} viewBox="0 0 9 9" className="inline-block align-[-1px]" aria-hidden="true">
      <circle
        cx={4.5}
        cy={4.5}
        r={hollow ? 3 : 3.5}
        className={className}
        stroke={hollow ? 'currentColor' : 'none'}
        strokeWidth={hollow ? 1.5 : 0}
      />
    </svg>
  );
}

function ScaleBar({ step, pixels }: { readonly step: number; readonly pixels: number }) {
  return (
    <span className="flex items-center gap-2 text-xs text-mesh-muted">
      <span
        className="block h-2 border-x border-b border-mesh-muted"
        style={{ width: `${Math.round(pixels)}px` }}
      />
      <span className="tabular">{formatDistance(step)}</span>
    </span>
  );
}

/**
 * The arrangement for a mesh that does not say where it is.
 *
 * Not a stand-in for the map: a mesh with fifty nodes of which two report
 * coordinates is the normal case, and this says more about it than two dots
 * in an empty field would.
 */
function Rings({
  nodes,
  now,
  size,
  selected,
  onSelect,
}: {
  readonly nodes: readonly GroundNode[];
  readonly now: number;
  readonly size: { readonly width: number; readonly height: number };
  readonly selected: string | null;
  readonly onSelect: (key: string | null) => void;
}) {
  const placed = rings(nodes, now);
  const centreX = size.width / 2;
  const centreY = size.height / 2;
  const radius = Math.max(Math.min(size.width, size.height) / 2 - PADDING, 40);
  const placeable = nodes.filter(isPlaceable).length;

  return (
    <>
      <svg
        width={size.width}
        height={size.height}
        className="block"
        role="img"
        aria-label={`Netzansicht mit ${placed.length} Knoten, Abstand nach Zwischenstationen`}
      >
        {[1, 2, 3, 4].map((ring) => (
          <circle
            key={ring}
            cx={centreX}
            cy={centreY}
            r={(radius * ring) / 4}
            className="fill-none stroke-mesh-border"
            strokeWidth={1}
            strokeDasharray="2 5"
          />
        ))}

        <circle cx={centreX} cy={centreY} r={6} className="fill-mesh-accent" />
        <text
          x={centreX}
          y={centreY + 22}
          textAnchor="middle"
          fontSize={11}
          className="fill-mesh-muted"
        >
          dieser Node
        </text>

        {placed.map((one) => {
          const x = centreX + Math.cos(one.angle) * radius * one.radius;
          const y = centreY + Math.sin(one.angle) * radius * one.radius;

          return (
            <g
              key={one.node.key}
              opacity={one.stale ? 0.45 : 1}
              role="button"
              tabIndex={0}
              className="cursor-pointer focus-visible:outline focus-visible:outline-2 focus-visible:outline-mesh-accent"
              onClick={() => onSelect(one.node.key === selected ? null : one.node.key)}
              onKeyDown={(event) => {
                if (event.key === 'Enter' || event.key === ' ') {
                  onSelect(one.node.key === selected ? null : one.node.key);
                }
              }}
            >
              {one.node.key === selected && (
                <circle
                  cx={x}
                  cy={y}
                  r={11}
                  className="fill-none stroke-mesh-accent"
                  strokeWidth={2}
                />
              )}
              <circle
                cx={x}
                cy={y}
                r={4}
                className={LOOKS[heard(one.node.lastSeen, now)].fill}
                stroke={heard(one.node.lastSeen, now) === 'lange' ? 'currentColor' : 'none'}
                strokeWidth={1.5}
              />
              <title>
                {one.node.name} ·{' '}
                {one.node.stations === null
                  ? 'kein Weg bekannt'
                  : `${one.hops} ${one.hops === 1 ? 'Strecke' : 'Strecken'}`}
              </title>
            </g>
          );
        })}
      </svg>

      <div className="pointer-events-none absolute bottom-4 left-4 max-w-md text-xs text-mesh-faint">
        Abstand vom Mittelpunkt heißt Zwischenstationen — die Richtung bedeutet nichts.
        {placeable === 1
          ? ' Ein Knoten meldet eine Position; ein Punkt ist noch keine Geografie.'
          : ' Kein Knoten meldet eine Position.'}{' '}
        Sobald zwei es tun, zeigt diese Fläche die Geografie.
      </div>
    </>
  );
}

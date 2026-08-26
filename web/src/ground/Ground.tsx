import { useMemo, useRef, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { useLiveReload, type AppEvent } from '../lib/events';
import { isAdvert } from '../lib/pushes';
import { useNow } from '../lib/useNow';
import { useResource } from '../lib/useResource';
import type { KnownContact } from '../modules/nodes/types';
import { useSize } from './useSize';
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

/** How far the reader has moved away from the view that fits everything. */
interface Adjust {
  readonly east: number;
  readonly north: number;
  readonly scale: number;
}

const UNTOUCHED: Adjust = { east: 0, north: 0, scale: 1 };

export function Ground() {
  const now = useNow();
  const [attach, size] = useSize();
  const contacts = useResource<KnownContact[]>('/nodes/contacts');
  const status = useResource<{ node_self: SelfNode | null }>('/system/status');

  useLiveReload(
    (event: AppEvent) => event.type === 'push' && isAdvert(event.payload),
    () => contacts.reload(),
  );

  const nodes = useMemo(
    () => assemble(contacts.data ?? [], status.data?.node_self ?? null),
    [contacts.data, status.data],
  );
  const geo = useMemo(() => geography(nodes, now), [nodes, now]);

  return (
    <div ref={attach} className="absolute inset-0 overflow-hidden bg-mesh-bg">
      {size.width > 0 &&
        (geo === null ? (
          <Rings nodes={nodes} now={now} size={size} />
        ) : (
          <Geography geo={geo} nodes={nodes} size={size} />
        ))}
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
  size,
}: {
  readonly geo: NonNullable<ReturnType<typeof geography>>;
  readonly nodes: readonly GroundNode[];
  readonly size: { readonly width: number; readonly height: number };
}) {
  const navigate = useNavigate();
  const [adjust, setAdjust] = useState<Adjust>(UNTOUCHED);
  // Not pointer capture: capturing makes the SVG the target of the following
  // click, so a click on a node would never reach the node. Instead the drag
  // is tracked here, and a click that came out of a drag is ignored — nobody
  // means to open a node by letting go of the map on top of it.
  const drag = useRef({ active: false, x: 0, y: 0, moved: false });
  const open = (key: string) => {
    if (drag.current.moved) return;
    navigate(`/knoten/${key}`);
  };

  // Derived rather than stored: the view that fits everything changes when
  // the window resizes or a node appears, and a stored view would keep
  // showing the old section without anybody having asked it to.
  const base = fit(geo, size.width, size.height, PADDING);
  const view: View = {
    east: base.east + adjust.east,
    north: base.north + adjust.north,
    metresPerPixel: base.metresPerPixel * adjust.scale,
  };

  // Not memoised: it closes over `view`, which is derived fresh every render,
  // so a memo would only ever hand back a stale closure.
  const wheel = (event: React.WheelEvent<SVGSVGElement>) => {
    const box = event.currentTarget.getBoundingClientRect();
    const zoomed = zoomAt(
      view,
      event.deltaY > 0 ? 1.2 : 1 / 1.2,
      event.clientX - box.left,
      event.clientY - box.top,
      size.width,
      size.height,
    );
    setAdjust({
      east: zoomed.east - base.east,
      north: zoomed.north - base.north,
      scale: zoomed.metresPerPixel / base.metresPerPixel,
    });
  };

  const placed = geo.placed.map((one) => ({
    one,
    at: onScreen(one, view, size.width, size.height),
  }));
  const labelled = declutter(placed);
  const step = scaleStep(size.width * view.metresPerPixel);
  const missing = nodes.filter((node) => !isPlaceable(node)).length;

  return (
    <>
      <svg
        width={size.width}
        height={size.height}
        className="block cursor-grab touch-none active:cursor-grabbing"
        role="img"
        aria-label={`Karte mit ${geo.placed.length} verorteten Knoten`}
        onWheel={wheel}
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
          setAdjust((previous) => ({
            ...previous,
            east: previous.east - dx * view.metresPerPixel,
            north: previous.north + dy * view.metresPerPixel,
          }));
        }}
        onPointerUp={() => {
          // `moved` deliberately survives: the click arrives after this.
          drag.current.active = false;
        }}
        onPointerLeave={() => {
          drag.current.active = false;
        }}
      >
        {placed.map(({ one, at }, index) => (
          <Node
            key={one.node.key}
            placed={one}
            x={at.x}
            y={at.y}
            label={labelled.has(index)}
            onOpen={() => open(one.node.key)}
          />
        ))}
      </svg>

      <ScaleBar step={step} pixels={step / view.metresPerPixel} />

      <div className="pointer-events-none absolute right-4 bottom-14 flex flex-col items-end gap-1 text-xs text-mesh-faint">
        <span>Norden ist oben · blass heißt: seit über einem Tag nicht gehört</span>
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
      </div>

      {adjust !== UNTOUCHED && (
        <button
          type="button"
          onClick={() => setAdjust(UNTOUCHED)}
          className="absolute right-4 bottom-4 rounded-md border border-mesh-border bg-mesh-surface/90 px-2.5 py-1 text-xs text-mesh-muted backdrop-blur hover:text-mesh-text focus-visible:outline focus-visible:outline-2 focus-visible:outline-mesh-accent"
        >
          alles zeigen
        </button>
      )}
    </>
  );
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

function Node({
  placed,
  x,
  y,
  label,
  onOpen,
}: {
  readonly placed: Placed;
  readonly x: number;
  readonly y: number;
  readonly label: boolean;
  readonly onOpen: () => void;
}) {
  const { node, stale } = placed;

  return (
    <g
      opacity={stale ? 0.45 : 1}
      role="button"
      tabIndex={0}
      className="cursor-pointer focus-visible:outline focus-visible:outline-2 focus-visible:outline-mesh-accent"
      onClick={onOpen}
      onKeyDown={(event) => {
        if (event.key === 'Enter' || event.key === ' ') onOpen();
      }}
    >
      {node.own && (
        <circle cx={x} cy={y} r={11} className="fill-none stroke-mesh-accent" strokeWidth={1} />
      )}
      <circle
        cx={x}
        cy={y}
        r={node.own ? 6 : 5}
        className={stale ? 'fill-mesh-border' : 'fill-mesh-accent'}
      />
      {label && (
        <text
          x={x}
          y={y - 12}
          textAnchor="middle"
          fontSize={11}
          className={stale ? 'fill-mesh-faint' : 'fill-mesh-muted'}
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

function ScaleBar({ step, pixels }: { readonly step: number; readonly pixels: number }) {
  return (
    <div className="pointer-events-none absolute bottom-4 left-4 flex items-center gap-2 text-xs text-mesh-muted">
      <span
        className="block h-2 border-x border-b border-mesh-muted"
        style={{ width: `${Math.round(pixels)}px` }}
      />
      <span className="tabular">{formatDistance(step)}</span>
    </div>
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
}: {
  readonly nodes: readonly GroundNode[];
  readonly now: number;
  readonly size: { readonly width: number; readonly height: number };
}) {
  const navigate = useNavigate();
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
              onClick={() => navigate(`/knoten/${one.node.key}`)}
              onKeyDown={(event) => {
                if (event.key === 'Enter' || event.key === ' ') {
                  navigate(`/knoten/${one.node.key}`);
                }
              }}
            >
              <circle
                cx={x}
                cy={y}
                r={4}
                className={one.stale ? 'fill-mesh-border' : 'fill-mesh-accent'}
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

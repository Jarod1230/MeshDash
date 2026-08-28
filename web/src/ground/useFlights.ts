import { useEffect, useRef, useState } from 'react';
import { useLiveEvent, type AppEvent } from '../lib/events';
import { resolve } from './links';
import { isPlaceable, toWorld, type GroundNode, type World } from './projection';

/** How long a packet takes to cross one leg, in milliseconds. */
const PER_LEG = 550;
/** The shortest a flight lasts, so a single leg is still visible. */
const SHORTEST = 700;
/**
 * How many packets may be in the air at once.
 *
 * A busy mesh can hear faster than an eye can follow. Past this the oldest go
 * — the count beside the layer switch stays the honest measure of volume, this
 * is only what it looks like.
 */
const AT_ONCE = 40;

/** One packet on its way, as far as its path could be followed. */
export interface Flight {
  readonly id: number;
  /** Where it went, in travel order, ending at this node. */
  readonly legs: readonly World[];
  readonly startedAt: number;
  /** How long the whole journey is drawn to take. */
  readonly duration: number;
  /** What it carries, so an advert can be told from a message. */
  readonly payloadType: number;
}

/** What `traffic` publishes for every packet it reads. */
interface Announced {
  readonly payload_type: number;
  readonly stations: readonly string[];
}

/**
 * Follows the longest run of stations that can be placed, ending at this node.
 *
 * Each station is a key prefix. Where one fits more than one known node, fits
 * none, or fits a node with no position, the journey cannot be drawn there —
 * so the run is cut and only its tail is kept. The tail is always the part we
 * know most about: it ends where the packet was actually received.
 */
export function follow(stations: readonly string[], nodes: readonly GroundNode[]): World[] {
  const own = nodes.find((node) => node.own);
  if (own === undefined || !isPlaceable(own)) return [];

  const chain: (GroundNode | null)[] = [
    ...stations.map((prefix) => {
      const key = resolve(prefix, nodes);
      const node = key === null ? undefined : nodes.find((one) => one.key === key);
      return node !== undefined && isPlaceable(node) ? node : null;
    }),
    own,
  ];

  // Walk back from this node until the chain breaks.
  const tail: GroundNode[] = [];
  for (let index = chain.length - 1; index >= 0; index -= 1) {
    const node = chain[index];
    if (node === null || node === undefined) break;
    tail.unshift(node);
  }

  // One point is a place, not a journey. A packet heard straight from its
  // sender has an empty path, and the sender is named only inside the
  // encrypted payload — there is nothing to draw.
  if (tail.length < 2) return [];

  return tail.map((node) => toWorld(node.latitude ?? 0, node.longitude ?? 0));
}

/**
 * Packets travelling across the map, live.
 *
 * This is what the packet log is for: a heard packet carries the path it took,
 * so it can be drawn walking that path. Nothing here is fetched — the flight
 * exists for a second and is gone.
 */
export function useFlights(nodes: readonly GroundNode[]): {
  readonly flights: readonly Flight[];
  /** The moment the current frame is drawing, so the drawing stays pure. */
  readonly frame: number;
} {
  const [flights, setFlights] = useState<readonly Flight[]>([]);
  const [frame, setFrame] = useState(0);
  const nodesRef = useRef(nodes);
  const nextId = useRef(0);

  // Kept current in an effect: the subscription must not be rebuilt every time
  // the contact list is fetched again.
  useEffect(() => {
    nodesRef.current = nodes;
  });

  useLiveEvent(
    (event: AppEvent) =>
      event.type === 'module' && event.module === 'traffic' && event.kind === 'packet',
    (event) => {
      const announced = event.data as Announced | undefined;
      if (announced === undefined) return;

      const legs = follow(announced.stations ?? [], nodesRef.current);
      if (legs.length < 2) return;

      nextId.current += 1;
      const startedAt = Date.now();
      // The clock is set here too, so the first frame draws the packet where
      // it actually is rather than at the stale moment of the last one.
      setFrame(startedAt);
      const flight: Flight = {
        id: nextId.current,
        legs,
        startedAt,
        duration: Math.max((legs.length - 1) * PER_LEG, SHORTEST),
        payloadType: announced.payload_type,
      };

      setFlights((current) => [...current, flight].slice(-AT_ONCE));
    },
  );

  // Movement, and only movement.
  //
  // The clock is state rather than something the drawing reads for itself.
  // Reading the time while rendering makes a component impure — React may
  // render at any moment or not at all, and the dots would move in steps
  // rather than smoothly. The loop runs only while something is in the air: a
  // map that repaints sixty times a second while nothing happens is a map that
  // empties a battery for nothing.
  useEffect(() => {
    if (flights.length === 0) return;

    let handle = 0;
    const step = () => {
      setFrame(Date.now());
      handle = window.requestAnimationFrame(step);
    };
    handle = window.requestAnimationFrame(step);

    return () => window.cancelAnimationFrame(handle);
  }, [flights.length]);

  // Arrival, on a clock that keeps running when nobody is looking.
  //
  // A browser freezes `requestAnimationFrame` in a tab that is not visible.
  // Left to it alone, flights never expire while the map sits in a background
  // tab: they pile up, and the operator comes back to packets frozen on the
  // map that arrived half an hour ago — a drawing that claims traffic which is
  // long past. Observed at 2026-08-28 with eight dots standing still.
  //
  // A timer is throttled in a hidden tab but it does keep firing, so this
  // sweeps up either way. Coarse on purpose: it decides when something is
  // gone, not where it is.
  useEffect(() => {
    if (flights.length === 0) return;

    const sweep = window.setInterval(() => {
      const now = Date.now();
      setFlights((current) => {
        const alive = current.filter((flight) => now - flight.startedAt < flight.duration);
        return alive.length === current.length ? current : alive;
      });
    }, 200);

    return () => window.clearInterval(sweep);
  }, [flights.length]);

  return { flights, frame };
}

/**
 * Where a flight has got to, as a point between two of its stations.
 *
 * Returns null once it has arrived, so a finished flight draws nothing while
 * it waits to be swept up.
 */
export function positionOf(flight: Flight, now: number): World | null {
  const elapsed = (now - flight.startedAt) / flight.duration;
  if (elapsed >= 1) return null;

  const legs = flight.legs.length - 1;
  const overall = Math.max(elapsed, 0) * legs;
  const leg = Math.min(Math.floor(overall), legs - 1);
  const within = overall - leg;

  const from = flight.legs[leg];
  const to = flight.legs[leg + 1];
  if (from === undefined || to === undefined) return null;

  return {
    x: from.x + (to.x - from.x) * within,
    y: from.y + (to.y - from.y) * within,
  };
}

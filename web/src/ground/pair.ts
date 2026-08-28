import { resolve, type HeardBy } from './links';
import type { GroundNode } from './projection';
import type { Trace } from '../modules/nodes/types';

/**
 * Everything that is known about one connection, kept apart by source.
 *
 * The map draws a single line per pair, because a line is what a reader can
 * take in. What is actually stored is finer than that, and this is where the
 * finer form comes back out:
 *
 * - **Direction.** Hearing is not symmetric. A hears B and B does not hear A
 *   happens constantly with LoRa — different antenna heights, different
 *   transmit power, a hill on one side. That asymmetry is usually the finding
 *   somebody is after, and the line alone hides it.
 * - **Source.** "We reached it directly", "a trace measured it" and "we
 *   overheard it in twelve packets" are three different claims of three
 *   different strengths.
 */

/** One direction of hearing, as it was overheard. */
export interface Overheard {
  /** Key of the station that transmitted. */
  readonly talker: string;
  /** Key of the station that heard it. */
  readonly listener: string;
  /** How many packets showed it. */
  readonly heard: number;
  /** When the pair was first and last seen, epoch milliseconds. */
  readonly first: number;
  readonly last: number;
  /** Bytes per prefix. One is a weak match, three is nearly certain. */
  readonly width: number;
}

/** One leg a trace walked, with what the far end reported for it. */
export interface Measured {
  /** Decibels, or null where the answer grouped several stations. */
  readonly snr: number | null;
  /** When the trace went out, epoch milliseconds. */
  readonly at: number;
}

/** What is known about the connection between two nodes. */
export interface PairFacts {
  readonly a: GroundNode;
  readonly b: GroundNode;
  /** Whether this node reaches the other without a station in between. */
  readonly direct: boolean;
  /** Overheard hearings, at most one entry per direction. */
  readonly overheard: readonly Overheard[];
  /** Legs a trace measured, newest first. */
  readonly measured: readonly Measured[];
}

/** The identity of a pair, whichever way round it was seen. */
export function pairId(a: string, b: string): string {
  return a < b ? `${a}|${b}` : `${b}|${a}`;
}

/**
 * Gathers what is known about one pair.
 *
 * Returns null when either end is not a node this instance knows — a line is
 * only drawn between two known nodes, so this is a link that was never on the
 * map.
 */
export function facts(
  id: string,
  nodes: readonly GroundNode[],
  traces: readonly Trace[],
  overheard: readonly HeardBy[],
): PairFacts | null {
  const [first, second] = id.split('|');
  const a = nodes.find((node) => node.key === first);
  const b = nodes.find((node) => node.key === second);
  if (a === undefined || b === undefined) return null;

  return {
    a,
    b,
    direct: (a.own && b.stations === 0) || (b.own && a.stations === 0),
    overheard: hearings(a, b, nodes, overheard),
    measured: legs(a, b, nodes, traces),
  };
}

/** The overheard hearings between two nodes, at most one per direction. */
function hearings(
  a: GroundNode,
  b: GroundNode,
  nodes: readonly GroundNode[],
  overheard: readonly HeardBy[],
): Overheard[] {
  const own = nodes.find((node) => node.own);
  // A node that receives a packet leaves no prefix in the path it received,
  // so it is written as the empty string.
  const named = (prefix: string) => (prefix === '' ? (own?.key ?? null) : resolve(prefix, nodes));

  const found = new Map<string, Overheard>();

  for (const pair of overheard) {
    const talker = named(pair.talker);
    const listener = named(pair.listener);
    if (talker === null || listener === null) continue;

    const between =
      (talker === a.key && listener === b.key) || (talker === b.key && listener === a.key);
    if (!between) continue;

    // The same direction can arrive under several prefix widths — the same
    // node written with one byte by one sender and two by another. They are
    // one hearing, and the counts belong together.
    const key = `${talker}>${listener}`;
    const standing = found.get(key);
    found.set(key, {
      talker,
      listener,
      heard: (standing?.heard ?? 0) + pair.heard,
      first: Math.min(standing?.first ?? Infinity, new Date(pair.first_seen).getTime()),
      last: Math.max(standing?.last ?? 0, new Date(pair.last_seen).getTime()),
      // The widest is the strongest claim of the two, so it is the one to
      // report on.
      width: Math.max(standing?.width ?? 0, pair.width),
    });
  }

  return [...found.values()];
}

/** The legs of traced routes that ran between these two nodes. */
function legs(
  a: GroundNode,
  b: GroundNode,
  nodes: readonly GroundNode[],
  traces: readonly Trace[],
): Measured[] {
  const own = nodes.find((node) => node.own);
  const found: Measured[] = [];

  for (const trace of traces) {
    if (trace.answered_at === null) continue;

    const chain: (string | null)[] = [
      own?.key ?? null,
      ...trace.hops.map((hop) => resolve(hop.key_prefix, nodes)),
      trace.public_key,
    ];

    for (let leg = 0; leg + 1 < chain.length; leg += 1) {
      const from = chain[leg];
      const to = chain[leg + 1];
      const between = (from === a.key && to === b.key) || (from === b.key && to === a.key);
      if (!between) continue;

      // `hop.snr` is how well that station heard the one before it, so it
      // belongs to the leg arriving there. The last leg has no station of its
      // own and therefore no measurement.
      found.push({
        snr: trace.hops[leg]?.snr ?? null,
        at: new Date(trace.asked_at).getTime(),
      });
    }
  }

  return found.sort((one, other) => other.at - one.at);
}

import { resolve, type Named } from './prefix';
import type { Trace } from '../modules/nodes/types';

/**
 * Who a node hears, and who hears it.
 *
 * # Two sources, and they are not equal
 *
 * **Overheard.** Every packet this instance receives carries the path it took,
 * and a forwarding station appends itself to the end of it. So each
 * neighbouring pair on that path heard each other. Nobody transmits for this —
 * it accumulates from listening. Its weakness is the naming: a station is one
 * to three bytes of a key, and at one byte several nodes fit.
 *
 * **Measured.** A traceroute walks a route and reports, per station, how well
 * it heard the one before it. That is the only measurement of how two *other*
 * stations hear each other — and it costs airtime, so it happens when somebody
 * asks for it.
 *
 * A neighbour query for telemetry is a third thing and is **not** in here: it
 * returns what a node says about itself, not how well a link carries.
 *
 * The two are kept apart rather than merged into one score. "Twelve packets
 * showed it" and "it measured −4 dB" answer different questions.
 */

/** What `/api/v1/traffic/links` answers: one observed "heard directly". */
export interface HeardBy {
  readonly talker: string;
  readonly listener: string;
  readonly width: number;
  readonly first_seen: string;
  readonly last_seen: string;
  readonly heard: number;
}

/** One direction of hearing between two nodes. */
export interface Direction {
  /** How many packets showed it. */
  readonly heard: number;
  /** First and last time it was seen, epoch milliseconds. */
  readonly first: number;
  readonly last: number;
  /** Widest prefix that named it. One byte is weak, three is nearly certain. */
  readonly width: number;
}

/** Everything known about one node's relationship to another. */
export interface Neighbour {
  /** The other node's key, and its name if this instance knows one. */
  readonly key: string;
  readonly name: string;
  /** Packets in which the other node was heard **by** the node in question. */
  readonly hears: Direction | null;
  /** Packets in which the node in question was heard by the other. */
  readonly heardBy: Direction | null;
  /** Legs a trace measured between the two, newest first. */
  readonly measured: readonly { readonly snr: number | null; readonly at: number }[];
}

/**
 * Collects every node that is known to hear, or be heard by, this one.
 *
 * Sorted by how much there is to go on: a neighbour seen in a hundred packets
 * says more about the mesh than one seen twice.
 */
export function neighbours(
  key: string,
  nodes: readonly Named[],
  traces: readonly Trace[],
  overheard: readonly HeardBy[],
): Neighbour[] {
  const own = nodes.find((node) => node.own);
  // A node that receives a packet leaves no prefix in the path it received,
  // so it is written as the empty string.
  const named = (prefix: string) => (prefix === '' ? (own?.key ?? null) : resolve(prefix, nodes));

  const found = new Map<string, { hears: Direction | null; heardBy: Direction | null }>();

  const note = (other: string, side: 'hears' | 'heardBy', pair: HeardBy) => {
    const entry = found.get(other) ?? { hears: null, heardBy: null };
    const standing = entry[side];

    // The same direction can arrive under several prefix widths — one sender
    // writes a byte, the next writes two. It is one relationship.
    found.set(other, {
      ...entry,
      [side]: {
        heard: (standing?.heard ?? 0) + pair.heard,
        first: Math.min(standing?.first ?? Infinity, new Date(pair.first_seen).getTime()),
        last: Math.max(standing?.last ?? 0, new Date(pair.last_seen).getTime()),
        width: Math.max(standing?.width ?? 0, pair.width),
      },
    });
  };

  for (const pair of overheard) {
    const talker = named(pair.talker);
    const listener = named(pair.listener);
    if (talker === null || listener === null || talker === listener) continue;

    if (listener === key) note(talker, 'hears', pair);
    else if (talker === key) note(listener, 'heardBy', pair);
  }

  const measured = legs(key, nodes, traces);
  for (const other of measured.keys()) {
    if (!found.has(other)) found.set(other, { hears: null, heardBy: null });
  }

  return [...found.entries()]
    .map(([other, sides]) => ({
      key: other,
      name: nodes.find((node) => node.key === other)?.name ?? other.slice(0, 6),
      hears: sides.hears,
      heardBy: sides.heardBy,
      measured: measured.get(other) ?? [],
    }))
    .sort((one, other) => weight(other) - weight(one));
}

/** How much there is to go on, for ordering. */
function weight(neighbour: Neighbour): number {
  return (neighbour.hears?.heard ?? 0) + (neighbour.heardBy?.heard ?? 0) + neighbour.measured.length;
}

/** The measured legs of traced routes that touched this node. */
function legs(
  key: string,
  nodes: readonly Named[],
  traces: readonly Trace[],
): Map<string, { snr: number | null; at: number }[]> {
  const own = nodes.find((node) => node.own);
  const found = new Map<string, { snr: number | null; at: number }[]>();

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
      if (from === null || to === null || from === undefined || to === undefined) continue;

      const other = from === key ? to : to === key ? from : null;
      if (other === null || other === key) continue;

      // `hop.snr` is how well that station heard the one before it, so it
      // belongs to the leg arriving there. The last leg has no station of its
      // own, and `final_snr` describes the answer coming back rather than that
      // leg — pinning it here would be an invention.
      found.set(other, [
        ...(found.get(other) ?? []),
        { snr: trace.hops[leg]?.snr ?? null, at: new Date(trace.asked_at).getTime() },
      ]);
    }
  }

  for (const list of found.values()) list.sort((one, other) => other.at - one.at);

  return found;
}

import { pairId, resolve } from '../lib/prefix';
import type { GroundNode } from './projection';
import type { Trace } from '../modules/nodes/types';

/** What `/api/v1/traffic/links` answers: one observed "heard directly". */
export interface HeardBy {
  /** Prefix of the station that transmitted, lowercase hex. */
  readonly talker: string;
  /** Prefix of the station that heard it. Empty means this node. */
  readonly listener: string;
  /** Bytes per prefix. One byte is a weak match, three is nearly certain. */
  readonly width: number;
  readonly first_seen: string;
  readonly last_seen: string;
  /** How many packets showed it. */
  readonly heard: number;
}

/**
 * Which connections in the mesh are established fact, and how well they carry.
 *
 * # Only what was actually observed
 *
 * Two sources, both of them measurements rather than inferences:
 *
 * - **Direct neighbours.** A contact the node reaches without a station in
 *   between. That the link exists is certain; how well it carries is not
 *   measured here, and is left unstated rather than guessed at.
 * - **Traced legs.** A trace walks a route and reports, per station, how well
 *   it heard the one before it. Those are the only measurements MeshDash has
 *   about how two *other* stations hear each other.
 * - **Overheard packets.** Every packet this node receives carries the path it
 *   travelled, and a forwarding station appends itself to the end of it. So
 *   each neighbouring pair on that path heard each other, and the last station
 *   was heard here. Nobody had to transmit for this — it accumulates from
 *   listening.
 *
 * Everything else stays undrawn. A route known from a contact's path is a list
 * of one-byte key prefixes, and in a mesh of any size two nodes share a first
 * byte — drawing a line from that would put a measurement's weight behind a
 * coin toss.
 */

/**
 * How a connection came to be known.
 *
 * Separate from whether it was measured: a leg a trace walked is established
 * fact even where the answer carried no value for it.
 */
export type LinkKind = 'direkt' | 'verfolgt' | 'gehört';

/** One connection between two nodes, as far as it was observed. */
export interface MeshLink {
  /** Stable identity of the pair, whichever way round it was seen. */
  readonly id: string;
  readonly from: string;
  readonly to: string;
  /**
   * How well the far end heard this leg, where it was measured.
   *
   * `null` for a direct neighbour — that the link exists is certain, how well
   * it carries was not measured. Saying nothing is the honest form of that.
   */
  readonly snr: number | null;
  readonly kind: LinkKind;
  /**
   * How many packets showed this pair, where that was counted.
   *
   * Only overheard links have it. Twenty-eight packets over a pair says more
   * about a link than one does, and it is the only strength this source can
   * honestly report.
   */
  readonly heard: number | null;
  /** When this was observed, epoch milliseconds. Newer wins over older. */
  readonly at: number;
}


/**
 * Collects every connection worth drawing.
 *
 * Where the same pair turns up twice, the measured observation beats the bare
 * one and the newer beats the older — a trace from this morning says more
 * about the mesh than one from last week.
 */
export function links(
  nodes: readonly GroundNode[],
  traces: readonly Trace[],
  overheard: readonly HeardBy[],
  now: number,
): MeshLink[] {
  const own = nodes.find((node) => node.own);
  const found = new Map<string, MeshLink>();

  // A measurement outranks a bare "it exists"; between two of a kind, the
  // newer one — a trace from this morning says more than one from last week.
  const outranks = (candidate: MeshLink, standing: MeshLink) => {
    const measured = candidate.snr !== null;
    if (measured !== (standing.snr !== null)) return measured;

    return candidate.at > standing.at;
  };

  const add = (link: MeshLink) => {
    const standing = found.get(link.id);
    if (standing === undefined || outranks(link, standing)) found.set(link.id, link);
  };

  if (own !== undefined) {
    for (const node of nodes) {
      if (node.own || node.stations !== 0) continue;
      add({
        id: pairId(own.key, node.key),
        from: own.key,
        to: node.key,
        snr: null,
        heard: null,
        kind: 'direkt',
        // The link is as current as the last time anything was heard from it.
        at: Math.min(node.lastSeen, now),
      });
    }
  }

  for (const trace of traces) {
    if (trace.answered_at === null) continue;
    const at = new Date(trace.asked_at).getTime();

    // The chain a packet walked: this node, every station in travel order,
    // then the node the trace was aimed at.
    const chain: (string | null)[] = [
      own?.key ?? null,
      ...trace.hops.map((hop) => resolve(hop.key_prefix, nodes)),
      trace.public_key,
    ];

    for (let leg = 0; leg + 1 < chain.length; leg += 1) {
      const from = chain[leg];
      const to = chain[leg + 1];
      // A station nobody could name breaks the two legs it touches, and only
      // those: the legs beyond it are still between nodes that are known.
      if (from === null || to === null || from === undefined || to === undefined) continue;
      if (from === to) continue;

      // `hop.snr` is how well that station heard the one before it, so it
      // belongs to the leg arriving there. The last leg has no station of its
      // own and therefore no measurement — `final_snr` describes the answer
      // coming back, not this leg, and pinning it here would be an invention.
      const snr = trace.hops[leg]?.snr ?? null;

      add({ id: pairId(from, to), from, to, snr, heard: null, kind: 'verfolgt', at });
    }
  }

  for (const pair of overheard) {
    // The empty listener is this node: a receiver leaves no prefix in the path
    // it received, so there is nothing to resolve.
    const listener = pair.listener === '' ? (own?.key ?? null) : resolve(pair.listener, nodes);
    const talker = resolve(pair.talker, nodes);
    if (talker === null || listener === null || talker === listener) continue;

    add({
      id: pairId(talker, listener),
      from: talker,
      to: listener,
      // The path says who forwarded, not how well it was received. The one
      // reception this node did measure belongs to a packet, not to the pair,
      // and averaging over a month of them would invent a number.
      snr: null,
      heard: pair.heard,
      kind: 'gehört',
      at: new Date(pair.last_seen).getTime(),
    });
  }

  return [...found.values()];
}

/**
 * How strongly a link is drawn, from how well it was heard.
 *
 * The range is the one LoRa lives in: below about -10 dB a link is barely
 * holding, above about +5 it is comfortable. Unmeasured links get the thinnest
 * stroke there is, so "we know it exists" never looks like "we measured it and
 * it is good".
 */
export function strokeFor(snr: number | null): number {
  if (snr === null) return 1.25;

  const scaled = (snr + 10) / 15;

  return 1.5 + 2.5 * Math.min(Math.max(scaled, 0), 1);
}

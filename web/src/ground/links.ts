import type { GroundNode } from './projection';
import type { Trace } from '../modules/nodes/types';

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
export type LinkKind = 'direkt' | 'verfolgt';

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
  /** When this was observed, epoch milliseconds. Newer wins over older. */
  readonly at: number;
}

/** The same pair, whichever direction it was seen from. */
function pairId(a: string, b: string): string {
  return a < b ? `${a}|${b}` : `${b}|${a}`;
}

/**
 * Which node a trace's key prefix refers to — if it can only be one.
 *
 * A station on a traced route is named by the leading bytes of its key, often
 * a single one. With more than one candidate, nobody is named: the same
 * birthday problem as a message's sender prefix, and the same answer.
 */
export function resolve(prefix: string, nodes: readonly GroundNode[]): string | null {
  if (prefix === '') return null;

  const candidates = nodes.filter((node) => node.key.startsWith(prefix.toLowerCase()));

  return candidates.length === 1 ? (candidates[0]?.key ?? null) : null;
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

      add({ id: pairId(from, to), from, to, snr, kind: 'verfolgt', at });
    }
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

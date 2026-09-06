/**
 * Matching the first bytes of a key back to a node.
 *
 * A packet names the stations on its path by the **leading bytes of their
 * public key** — one to three, chosen by whoever sent it. Everything that
 * reasons about who heard whom starts here, which is why this sits in `lib`
 * rather than beside any one view.
 */

/** The least a caller has to know about a node for this to work. */
export interface Named {
  readonly key: string;
  readonly name: string;
  readonly own: boolean;
}

/**
 * Which node a prefix refers to — if it can only be one.
 *
 * With more than one candidate, nobody is named. At one byte that is 256
 * values against however many nodes the mesh has, so two sharing a first byte
 * is the normal case rather than bad luck: the same birthday problem as a
 * message's sender prefix, and the same answer.
 */
export function resolve(prefix: string, nodes: readonly Named[]): string | null {
  if (prefix === '') return null;

  const candidates = nodes.filter((node) => node.key.startsWith(prefix.toLowerCase()));

  return candidates.length === 1 ? (candidates[0]?.key ?? null) : null;
}

/** The identity of a pair of nodes, whichever way round it was seen. */
export function pairId(a: string, b: string): string {
  return a < b ? `${a}|${b}` : `${b}|${a}`;
}

/** What `/api/v1/nodes/contacts` answers. */
/** Where a node's position comes from. */
export type PositionSource = 'reported' | 'manual';

export interface KnownContact {
  readonly public_key: string;
  readonly name: string;
  readonly contact_type: number;
  readonly flags: number;
  /** Where the position that applies comes from, or null without one. */
  readonly position_source: PositionSource | null;
  /** What the node itself claims, even where a set position overrides it. */
  readonly reported_latitude: number | null;
  readonly reported_longitude: number | null;
  /**
   * The known route as hex hop bytes, or `null` when the node has no route to
   * this contact.
   *
   * An empty string is not the same: it means reachable directly.
   */
  readonly path: string | null;
  /**
   * How many stations the route passes through, or `null` without a route.
   *
   * Not the same as the length of `path`: a station can take more than one
   * byte on the wire. Counting bytes gave hop numbers that were quietly wrong
   * — see `meshdash_proto::path`.
   */
  readonly stations: number | null;
  readonly latitude: number | null;
  readonly longitude: number | null;
  readonly last_advert: number;
  readonly first_seen: string;
  readonly last_seen: string;
}

/** What `/api/v1/nodes/adverts` answers. */
export interface Sighting {
  /** Running number, ascending with arrival. Cursor for the next page. */
  readonly id: number;
  readonly public_key: string;
  readonly heard_at: string;
  readonly was_new: boolean;
}

/**
 * How a route reads in the interface.
 *
 * `null` stations means the node has no route to this contact — which is not
 * the same as a route with none in between, and treating them alike is how
 * every node ended up on the outermost ring of the topology view.
 */
export function describeRoute(stations: number | null): string {
  if (stations === null) return 'Weg unbekannt';
  if (stations === 0) return 'direkt';
  return stations === 1 ? '1 Station' : `${stations} Stationen`;
}

/** One recorded change of the route to a node. */
export interface RouteChange {
  readonly id: number;
  readonly public_key: string;
  readonly changed_at: string;
  readonly path: string | null;
  readonly stations: number | null;
  readonly previous_path: string | null;
  readonly previous_stations: number | null;
}

/** A key is 64 hex digits; nobody reads that. Six are enough to tell apart. */
export function shortKey(publicKey: string): string {
  return publicKey.slice(0, 6);
}

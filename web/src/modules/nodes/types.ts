/** What `/api/v1/nodes/contacts` answers. */
export interface KnownContact {
  readonly public_key: string;
  readonly name: string;
  readonly contact_type: number;
  readonly flags: number;
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

/** A key is 64 hex digits; nobody reads that. Six are enough to tell apart. */
export function shortKey(publicKey: string): string {
  return publicKey.slice(0, 6);
}

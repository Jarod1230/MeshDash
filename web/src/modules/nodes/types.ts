/** What `/api/v1/nodes/contacts` answers. */
export interface KnownContact {
  readonly public_key: string;
  readonly name: string;
  readonly contact_type: number;
  readonly flags: number;
  /** The known route as hex hop bytes; empty means a direct neighbour. */
  readonly path: string;
  readonly latitude: number | null;
  readonly longitude: number | null;
  readonly last_advert: number;
  readonly first_seen: string;
  readonly last_seen: string;
}

/** What `/api/v1/nodes/adverts` answers. */
export interface Sighting {
  readonly public_key: string;
  readonly heard_at: string;
  readonly was_new: boolean;
}

/** Hops in a stored path. Two hex digits per hop. */
export function hopCount(path: string): number {
  return Math.floor(path.length / 2);
}

/** A key is 64 hex digits; nobody reads that. Six are enough to tell apart. */
export function shortKey(publicKey: string): string {
  return publicKey.slice(0, 6);
}

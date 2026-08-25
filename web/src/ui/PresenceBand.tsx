import { exactTime } from '../lib/time';

/** One stretch of time and how often the node was heard within it. */
export interface Bucket {
  readonly from: string;
  readonly to: string;
  readonly sightings: number;
}

/** What `/api/v1/nodes/presence` answers. */
export interface Presence {
  readonly from: string;
  readonly to: string;
  readonly buckets: readonly Bucket[];
}

/**
 * How reachable a node has been, as one band of equal stretches.
 *
 * A list of sightings answers "when was it heard". Over a week that list is
 * hundreds of rows and nobody reads it. The band answers the question those
 * rows were being scanned for: was this node there the whole time, or does it
 * come and go — and the shape of the gaps says which.
 *
 * Brightness carries how often, not a rank: two stretches with 3 and 4
 * sightings should look alike, while silence must be unmistakable.
 */
export function PresenceBand({ presence }: { readonly presence: Presence }) {
  const loudest = Math.max(1, ...presence.buckets.map((bucket) => bucket.sightings));

  return (
    <div>
      <div className="flex h-10 gap-px" role="img" aria-label={describe(presence)}>
        {presence.buckets.map((bucket) => (
          <div
            key={bucket.from}
            className="min-w-0 flex-1 rounded-[1px]"
            style={{
              backgroundColor:
                bucket.sightings === 0
                  ? 'var(--color-mesh-raised)'
                  : `color-mix(in oklch, var(--color-mesh-accent) ${share(bucket.sightings, loudest)}%, var(--color-mesh-raised))`,
            }}
            title={`${exactTime(bucket.from)} — ${bucket.sightings === 0 ? 'still' : `${bucket.sightings}×`}`}
          />
        ))}
      </div>
      <div className="mt-1 flex justify-between text-xs text-mesh-faint">
        <span className="tabular">{exactTime(presence.from)}</span>
        <span className="tabular">{exactTime(presence.to)}</span>
      </div>
    </div>
  );
}

/**
 * How bright one stretch is, between a visible floor and full.
 *
 * The floor matters: a single sighting in a week of heavy traffic would
 * otherwise be indistinguishable from silence, and "heard once" is the
 * opposite of "not heard".
 */
export function share(sightings: number, loudest: number): number {
  if (sightings === 0) return 0;
  return Math.round(35 + (sightings / loudest) * 65);
}

/** What the band says, for whoever cannot see it. */
function describe(presence: Presence): string {
  const heard = presence.buckets.filter((bucket) => bucket.sightings > 0).length;
  const total = presence.buckets.length;
  return `In ${heard} von ${total} Abschnitten des Zeitraums gehört`;
}

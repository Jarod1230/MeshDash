import { exactTime, relativeTime } from '../lib/time';

/**
 * The link over time, as one band.
 *
 * "Connected" is a poor answer to the question people actually have. A node
 * that drops every two minutes reports itself connected each time it comes
 * back, and the status line looks identical to a link that has held for a
 * week. The band shows the difference: solid means held, notched means it
 * dropped, and the notches sit where they happened.
 *
 * Segments are proportional to real time, so a two-second dropout is a hair
 * and an hour offline is a gap — the same reading a chart recorder gives.
 */
export interface Change {
  /** Running number, ascending with arrival. Cursor for the next page. */
  readonly id: number;
  readonly at: string;
  readonly connected: boolean;
  readonly reason: string | null;
}

interface Segment {
  readonly connected: boolean;
  readonly seconds: number;
  readonly from: string;
  readonly reason: string | null;
}

/** Turns the change log into spans of time, oldest first. */
export function toSegments(changes: readonly Change[], now: number): Segment[] {
  if (changes.length === 0) return [];

  // The API answers newest first; a timeline reads the other way.
  const ordered = [...changes].reverse();
  const segments: Segment[] = [];

  for (const [index, change] of ordered.entries()) {
    const start = new Date(change.at).getTime();
    if (Number.isNaN(start)) continue;

    const nextEntry = ordered[index + 1];
    const end = nextEntry === undefined ? now : new Date(nextEntry.at).getTime();
    const seconds = Math.max(0, (end - start) / 1000);

    segments.push({
      connected: change.connected,
      seconds,
      from: change.at,
      reason: change.reason,
    });
  }

  return segments;
}

export function LinkBand({
  changes,
  now,
}: {
  readonly changes: readonly Change[];
  readonly now: number;
}) {
  const segments = toSegments(changes, now);

  if (segments.length === 0) {
    return (
      <p className="text-xs text-mesh-faint">
        Noch kein Verlauf aufgezeichnet — er entsteht ab der ersten Verbindung.
      </p>
    );
  }

  const total = segments.reduce((sum, segment) => sum + segment.seconds, 0) || 1;
  const oldest = segments[0];

  return (
    <div>
      <div className="flex h-2 w-full overflow-hidden rounded-full bg-mesh-border" role="img"
        aria-label={`Verbindungsverlauf über ${relativeTime(oldest?.from ?? '', new Date(now))}`}>
        {segments.map((segment) => (
          <span
            key={segment.from}
            // A dropout of two seconds must stay visible, so every segment
            // keeps a minimum width even when its share rounds to nothing.
            style={{ flexGrow: Math.max(segment.seconds / total, 0.004) }}
            className={segment.connected ? 'bg-mesh-accent' : 'bg-mesh-bad'}
            title={`${segment.connected ? 'Verbunden' : 'Getrennt'} ab ${exactTime(segment.from)}${
              segment.reason === null ? '' : ` — ${segment.reason}`
            }`}
          />
        ))}
      </div>
      <div className="mt-1.5 flex justify-between text-xs text-mesh-faint">
        <span>{oldest === undefined ? '' : relativeTime(oldest.from, new Date(now))}</span>
        <span>jetzt</span>
      </div>
    </div>
  );
}

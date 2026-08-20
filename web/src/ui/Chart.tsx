/**
 * A line over time, drawn by hand.
 *
 * No chart library, by decision (ADR-0008): one line with an axis is a few
 * dozen lines of SVG, where a library would weigh about as much as the rest
 * of the bundle together.
 *
 * # What this chart refuses to do
 *
 * It does not connect across gaps. If the node was offline for six hours,
 * joining the readings on either side would draw a smooth line through a
 * period where nothing was measured — an invented measurement, which is
 * exactly the kind of quiet falsehood this project avoids elsewhere. Gaps
 * larger than `gapSeconds` break the line instead.
 */
export interface Point {
  /** Milliseconds since the epoch. */
  readonly t: number;
  readonly value: number;
}

const WIDTH = 720;
const HEIGHT = 180;
const PAD_LEFT = 44;
const PAD_BOTTOM = 22;
const PAD_TOP = 10;

/** Splits a series wherever it stops being continuous. */
export function toRuns(points: readonly Point[], gapSeconds: number): Point[][] {
  const sorted = [...points].sort((a, b) => a.t - b.t);
  const runs: Point[][] = [];
  let current: Point[] = [];

  for (const [index, point] of sorted.entries()) {
    const previous = sorted[index - 1];
    if (previous !== undefined && (point.t - previous.t) / 1000 > gapSeconds) {
      if (current.length > 0) runs.push(current);
      current = [];
    }
    current.push(point);
  }

  if (current.length > 0) runs.push(current);
  return runs;
}

export function Chart({
  points,
  unit,
  gapSeconds = 3 * 3600,
  format = (value: number) => value.toFixed(0),
}: {
  readonly points: readonly Point[];
  readonly unit: string;
  readonly gapSeconds?: number;
  readonly format?: (value: number) => string;
}) {
  if (points.length < 2) {
    return (
      <p className="px-4 py-6 text-sm text-mesh-muted">
        Für eine Kurve braucht es mindestens zwei Messwerte. Bisher liegen{' '}
        {points.length === 0 ? 'keine' : 'ein Wert'} vor.
      </p>
    );
  }

  const values = points.map((point) => point.value);
  const times = points.map((point) => point.t);
  const minValue = Math.min(...values);
  const maxValue = Math.max(...values);
  const minTime = Math.min(...times);
  const maxTime = Math.max(...times);

  // A flat line would divide by zero and, worse, would be drawn at the very
  // edge of the box where it looks like an error.
  const span = maxValue - minValue || Math.max(Math.abs(maxValue) * 0.1, 1);
  const low = maxValue === minValue ? minValue - span / 2 : minValue;
  const timeSpan = maxTime - minTime || 1;

  const x = (t: number) => PAD_LEFT + ((t - minTime) / timeSpan) * (WIDTH - PAD_LEFT - 8);
  const y = (value: number) =>
    HEIGHT - PAD_BOTTOM - ((value - low) / span) * (HEIGHT - PAD_BOTTOM - PAD_TOP);

  const runs = toRuns(points, gapSeconds);
  const ticks = [low + span, low + span / 2, low];

  return (
    <figure className="p-4">
      <svg
        viewBox={`0 0 ${WIDTH} ${HEIGHT}`}
        className="block h-auto w-full"
        role="img"
        aria-label={`Verlauf von ${format(minValue)} bis ${format(maxValue)} ${unit}, ${points.length} Messwerte`}
      >
        {ticks.map((tick) => (
          <g key={tick}>
            <line
              x1={PAD_LEFT}
              y1={y(tick)}
              x2={WIDTH - 8}
              y2={y(tick)}
              className="stroke-mesh-border"
              strokeWidth={1}
            />
            <text
              x={PAD_LEFT - 8}
              y={y(tick) + 3}
              textAnchor="end"
              fontSize={10}
              className="fill-mesh-faint tabular"
            >
              {format(tick)}
            </text>
          </g>
        ))}

        {runs.map((run) => (
          <polyline
            key={run[0]?.t}
            points={run.map((point) => `${x(point.t)},${y(point.value)}`).join(' ')}
            fill="none"
            className="stroke-mesh-accent"
            strokeWidth={1.5}
            strokeLinejoin="round"
          />
        ))}

      </svg>

      <figcaption className="mt-1 space-y-1 text-xs text-mesh-faint">
        <div className="flex justify-between gap-2">
          <span className="tabular">{new Date(minTime).toLocaleString('de-DE')}</span>
          <span className="tabular">
            {points.length} Werte · {unit}
          </span>
          <span className="tabular">{new Date(maxTime).toLocaleString('de-DE')}</span>
        </div>
        {runs.length > 1 && (
          // Said in words, because an unexplained break in a line reads as a
          // rendering fault rather than as "nothing was measured here".
          <p>
            Die Linie ist {runs.length - 1}
            {runs.length === 2 ? ' Mal' : ' Mal'} unterbrochen — dort wurde nichts gemessen.
          </p>
        )}
      </figcaption>
    </figure>
  );
}

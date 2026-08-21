import { RANGES, type ChosenRange } from '../lib/timeRange';

/**
 * Which stretch of time a page shows.
 *
 * Fixed steps rather than two date fields: reading a curve is a matter of
 * zooming in and out, and picking two dates for that is three interactions
 * where one would do. An exact range belongs to an export, not to a glance.
 */
export function RangePicker({ range, label }: { readonly range: ChosenRange; readonly label: string }) {
  return (
    <div className="flex flex-wrap items-center gap-1" role="group" aria-label={label}>
      {RANGES.map((option) => (
        <button
          key={option.key}
          type="button"
          aria-pressed={range.key === option.key}
          onClick={() => range.choose(option.key)}
          className={`rounded-md border px-2 py-0.5 text-xs focus-visible:outline focus-visible:outline-2 focus-visible:outline-mesh-accent ${
            range.key === option.key
              ? 'border-mesh-accent text-mesh-text'
              : 'border-mesh-border text-mesh-muted hover:text-mesh-text'
          }`}
        >
          {option.label}
        </button>
      ))}
    </div>
  );
}

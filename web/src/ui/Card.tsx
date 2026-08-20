import type { ReactNode } from 'react';

/** A titled surface. The one container shape the interface uses. */
export function Card({
  title,
  hint,
  children,
}: {
  readonly title: string;
  readonly hint?: string;
  readonly children: ReactNode;
}) {
  return (
    <section className="rounded-lg border border-mesh-border bg-mesh-surface">
      <header className="flex items-baseline gap-3 border-b border-mesh-border px-4 py-2.5">
        <h2 className="text-sm text-mesh-text">{title}</h2>
        {hint !== undefined && <span className="text-xs text-mesh-faint">{hint}</span>}
      </header>
      {children}
    </section>
  );
}

/**
 * One measurement, large.
 *
 * The unit is set apart from the figure so the eye lands on the number first —
 * on a dashboard the value is scanned, the unit is only confirmed.
 */
export function Stat({
  label,
  value,
  unit,
  hint,
  tone = 'plain',
}: {
  readonly label: string;
  readonly value: string;
  readonly unit?: string;
  readonly hint?: string;
  readonly tone?: 'plain' | 'signal' | 'bad';
}) {
  const toneClass =
    tone === 'signal' ? 'text-mesh-accent' : tone === 'bad' ? 'text-mesh-bad' : 'text-mesh-text';

  return (
    <div className="rounded-lg border border-mesh-border bg-mesh-surface px-4 py-3">
      <div className="text-xs uppercase tracking-wider text-mesh-muted">{label}</div>
      <div className={`tabular mt-1 text-2xl leading-none ${toneClass}`}>
        {value}
        {unit !== undefined && <span className="ml-1 text-sm text-mesh-faint">{unit}</span>}
      </div>
      {hint !== undefined && <div className="mt-1 text-xs text-mesh-faint">{hint}</div>}
    </div>
  );
}

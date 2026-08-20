/**
 * How reception quality is shown, everywhere it appears.
 *
 * SNR in dB is not a number most people read fluently — is −3 good? So it is
 * always shown twice: as bars anyone can judge at a glance, and as the exact
 * figure for anyone who can. The scale below is a reading aid, not a protocol
 * fact: the firmware defines no thresholds, and none are claimed here.
 */

/** Bars lit for a given SNR. Four steps, from barely there to solid. */
function barsFor(snr: number): number {
  if (snr >= 8) return 4;
  if (snr >= 2) return 3;
  if (snr >= -5) return 2;
  return 1;
}

/** Colour for a given SNR: cyan means workable, amber means marginal. */
function toneFor(snr: number): string {
  if (snr >= 2) return 'bg-mesh-accent';
  if (snr >= -5) return 'bg-mesh-accent-dim';
  return 'bg-mesh-warn';
}

const HEIGHTS = ['h-1', 'h-2', 'h-3', 'h-4'];

export function SignalBars({ snr }: { readonly snr: number | null }) {
  if (snr === null) {
    return (
      <span className="text-mesh-faint" title="Keine Empfangsqualität gemeldet">
        —
      </span>
    );
  }

  const lit = barsFor(snr);
  const tone = toneFor(snr);

  return (
    <span
      className="inline-flex items-end gap-[2px]"
      role="img"
      aria-label={`Empfangsqualität ${snr.toFixed(1)} Dezibel`}
    >
      {HEIGHTS.map((height, index) => (
        <span
          key={height}
          className={`w-[3px] rounded-[1px] ${height} ${index < lit ? tone : 'bg-mesh-border'}`}
        />
      ))}
    </span>
  );
}

/** The figure itself, aligned in a column and never dancing. */
export function SignalValue({ snr }: { readonly snr: number | null }) {
  if (snr === null) return <span className="tabular text-mesh-faint">—</span>;

  const tone = snr >= 2 ? 'text-mesh-accent' : snr >= -5 ? 'text-mesh-text' : 'text-mesh-warn';
  // An explicit plus sign: without it "5.5 dB" and "−5.5 dB" look alike in a
  // scanned column, and the difference is the whole point.
  const sign = snr > 0 ? '+' : '';

  return (
    <span className={`tabular ${tone}`}>
      {sign}
      {snr.toFixed(1)} dB
    </span>
  );
}

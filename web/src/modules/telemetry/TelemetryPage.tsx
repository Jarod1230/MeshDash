import { Chart, type Point } from '../../ui/Chart';
import { SignalBars } from '../../ui/Signal';
import { Empty, Failed, Loading } from '../../ui/States';
import { useLiveReload, type AppEvent } from '../../lib/events';
import { useNow } from '../../lib/useNow';
import { useResource } from '../../lib/useResource';
import { exactTime, relativeTime } from '../../lib/time';
import { typeName, typeUnit } from './lppTypes';

/** What `/api/v1/telemetry/battery` answers. */
interface BatterySample {
  readonly at: string;
  readonly millivolts: number;
  readonly storage_used_kib: number;
  readonly storage_total_kib: number;
}

/** What `/api/v1/telemetry/neighbours` answers. */
interface NeighbourSample {
  readonly public_key: string;
  readonly at: string;
  readonly channel: number;
  readonly type_code: number;
  readonly value: number | null;
  readonly axes: readonly [number, number, number] | null;
  readonly position: readonly [number, number, number] | null;
}

/** What `/api/v1/telemetry/signal` answers. */
interface SignalSample {
  readonly at: string;
  readonly source: string;
  readonly snr: number;
  readonly path_len: number | null;
}

/**
 * How the node is doing, over time.
 *
 * Two series with different origins, which is worth knowing while reading
 * them: the battery is polled every five minutes and is therefore evenly
 * spaced, while reception quality only exists when someone transmitted — its
 * gaps are not failures, they are silence.
 */
export function TelemetryPage() {
  const now = useNow();
  const battery = useResource<BatterySample[]>('/telemetry/battery?limit=500');
  const neighbours = useResource<NeighbourSample[]>('/telemetry/neighbours?limit=200');
  const signal = useResource<SignalSample[]>('/telemetry/signal?limit=500');

  // Every stored message announces its reception quality on the bus.
  useLiveReload(
    (event: AppEvent) => event.type === 'module' && event.module === 'messages' && event.kind === 'signal',
    () => signal.reload(),
  );

  if (battery.error !== null && battery.data === null) {
    return (
      <div className="rounded-lg border border-mesh-border bg-mesh-surface">
        <Failed error={battery.error} onRetry={battery.reload} />
      </div>
    );
  }

  const latest = battery.data?.[0];
  const batteryPoints: Point[] =
    battery.data?.map((sample) => ({
      t: new Date(sample.at).getTime(),
      value: sample.millivolts / 1000,
    })) ?? [];
  const signalPoints: Point[] =
    signal.data?.map((sample) => ({ t: new Date(sample.at).getTime(), value: sample.snr })) ?? [];

  const storageShare =
    latest === undefined || latest.storage_total_kib === 0
      ? null
      : Math.round((latest.storage_used_kib / latest.storage_total_kib) * 100);

  return (
    <div className="space-y-4">
      <dl className="grid gap-3 sm:grid-cols-3">
        <Figure
          label="Batterie"
          value={latest === undefined ? '—' : (latest.millivolts / 1000).toFixed(2)}
          unit="V"
          hint={latest === undefined ? undefined : `gemessen ${relativeTime(latest.at, new Date(now))}`}
          accent
        />
        <Figure
          label="Speicher belegt"
          value={storageShare === null ? '—' : String(storageShare)}
          unit="%"
          hint={
            latest === undefined
              ? undefined
              : `${latest.storage_used_kib} von ${latest.storage_total_kib} KiB`
          }
        />
        <Figure
          label="Empfangsqualität"
          value={signalPoints.length === 0 ? '—' : String(signalPoints.length)}
          unit="Werte"
          hint="einer je Nachricht"
        />
      </dl>

      <section className="rounded-lg border border-mesh-border bg-mesh-surface">
        <header className="flex items-baseline gap-3 border-b border-mesh-border px-4 py-2.5">
          <h2 className="text-sm text-mesh-text">Batterie</h2>
          <span className="text-xs text-mesh-faint">alle fünf Minuten gemessen</span>
        </header>
        {battery.data === null ? (
          <Loading what="Die Batteriewerte" />
        ) : (
          <Chart
            points={batteryPoints}
            unit="Volt"
            format={(value) => value.toFixed(2)}
          />
        )}
      </section>

      <section className="rounded-lg border border-mesh-border bg-mesh-surface">
        <header className="flex items-baseline gap-3 border-b border-mesh-border px-4 py-2.5">
          <h2 className="text-sm text-mesh-text">Empfangsqualität</h2>
          <span className="text-xs text-mesh-faint">
            entsteht nur, wenn jemand sendet — Lücken sind Funkstille, kein Ausfall
          </span>
        </header>
        {signal.data === null ? (
          <Loading what="Die Empfangswerte" />
        ) : signalPoints.length === 0 ? (
          <Empty>
            Noch nichts empfangen. Jede eintreffende Nachricht bringt einen Messwert mit.
          </Empty>
        ) : (
          <>
            <Chart
              points={signalPoints}
              unit="dB"
              gapSeconds={6 * 3600}
              format={(value) => value.toFixed(1)}
            />
            <ul className="divide-y divide-mesh-border border-t border-mesh-border text-sm">
              {(signal.data ?? []).slice(0, 8).map((sample) => (
                <li
                  key={`${sample.at}-${sample.snr}`}
                  className="flex items-center gap-3 px-4 py-2"
                >
                  <SignalBars snr={sample.snr} />
                  <span className="tabular text-mesh-text">{sample.snr.toFixed(1)} dB</span>
                  <span className="text-xs text-mesh-muted">
                    {sample.source === 'direct' ? 'Direktnachricht' : 'Kanal'}
                    {sample.path_len === null
                      ? ', direkt empfangen'
                      : `, über ${sample.path_len} ${sample.path_len === 1 ? 'Station' : 'Stationen'}`}
                  </span>
                  <span className="tabular ml-auto text-xs text-mesh-faint">
                    {relativeTime(sample.at, new Date(now))}
                  </span>
                </li>
              ))}
            </ul>
          </>
        )}
      </section>

      <section className="rounded-lg border border-mesh-border bg-mesh-surface">
        <header className="flex flex-wrap items-baseline gap-3 border-b border-mesh-border px-4 py-2.5">
          <h2 className="text-sm text-mesh-text">Andere Knoten</h2>
          <span className="text-xs text-mesh-faint">
            was Nachbarn über sich selbst gemeldet haben
          </span>
        </header>
        {neighbours.data === null ? (
          <Loading what="Die Werte anderer Knoten" />
        ) : neighbours.data.length === 0 ? (
          <Empty>
            Bisher hat kein anderer Knoten etwas gemeldet. Danach gefragt wird nur, wenn{' '}
            <code className="text-mesh-accent">[modules.telemetry] neighbours</code> eingeschaltet
            ist — jede Anfrage belegt Sendezeit im gemeinsamen Band.
          </Empty>
        ) : (
          <ul className="divide-y divide-mesh-border text-sm">
            {neighbours.data.slice(0, 30).map((sample) => (
              <li
                key={`${sample.public_key}-${sample.at}-${sample.channel}-${sample.type_code}`}
                className="flex flex-wrap items-baseline gap-x-3 gap-y-1 px-4 py-2"
              >
                <span className="tabular text-xs text-mesh-faint">
                  {sample.public_key.slice(0, 6)}
                </span>
                <span className="text-mesh-muted">{typeName(sample.type_code)}</span>
                <span className="tabular text-mesh-text">{describe(sample)}</span>
                {sample.channel !== 1 && (
                  <span className="text-xs text-mesh-faint">Sensor {sample.channel}</span>
                )}
                <span
                  className="tabular ml-auto text-xs text-mesh-muted"
                  title={exactTime(sample.at)}
                >
                  {relativeTime(sample.at, new Date(now))}
                </span>
              </li>
            ))}
          </ul>
        )}
      </section>
    </div>
  );
}

/** One reading in words, whichever shape it has. */
function describe(sample: NeighbourSample): string {
  const unit = typeUnit(sample.type_code);

  if (sample.position !== null) {
    const [latitude, longitude, altitude] = sample.position;
    return `${latitude.toFixed(4)}°, ${longitude.toFixed(4)}°, ${altitude.toFixed(0)} m`;
  }

  if (sample.axes !== null) {
    return sample.axes.map((axis) => axis.toFixed(2)).join(' / ') + (unit === '' ? '' : ` ${unit}`);
  }

  if (sample.value === null) return '—';

  return `${sample.value.toFixed(2)}${unit === '' ? '' : ` ${unit}`}`;
}

function Figure({
  label,
  value,
  unit,
  hint,
  accent = false,
}: {
  readonly label: string;
  readonly value: string;
  readonly unit: string;
  readonly hint?: string | undefined;
  readonly accent?: boolean;
}) {
  return (
    <div className="rounded-lg border border-mesh-border bg-mesh-surface px-4 py-3">
      <dt className="text-xs uppercase tracking-wider text-mesh-muted">{label}</dt>
      <dd className={`tabular mt-1 text-2xl leading-none ${accent ? 'text-mesh-accent' : 'text-mesh-text'}`}>
        {value}
        <span className="ml-1 text-sm text-mesh-faint">{unit}</span>
      </dd>
      {hint !== undefined && <p className="mt-1 text-xs text-mesh-faint">{hint}</p>}
    </div>
  );
}

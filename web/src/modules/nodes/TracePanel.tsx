import { useState } from 'react';
import { apiPost, describeError, type ApiError } from '../../lib/api';
import { Empty, Loading } from '../../ui/States';
import { exactTime, relativeTime } from '../../lib/time';
import { SignalValue } from '../../ui/Signal';
import type { KnownContact, Trace } from './types';

/**
 * Walking a route station by station, and what each leg sounded like.
 *
 * This is the only way to learn how two *other* nodes hear each other: every
 * other measurement in MeshDash is about reception at this node. It costs
 * airtime — the packet travels to the far end and back — so it happens when
 * somebody asks for it and never on a timer.
 */
export function TracePanel({
  contact,
  traces,
  now,
  onStarted,
}: {
  readonly contact: KnownContact;
  readonly traces: readonly Trace[] | null;
  readonly now: number;
  readonly onStarted: () => void;
}) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Three cases, not two: a route with stations can be measured, a direct
  // one has nothing in between, and an unknown one is not the same as either.
  // Saying "directly reachable" for an unknown route claims something the
  // node never said.
  const traceable = contact.stations !== null && contact.stations > 0;
  const why =
    contact.stations === null
      ? 'Zu diesem Knoten ist kein Weg bekannt. Was sich nicht ablaufen lässt, lässt sich auch nicht messen.'
      : contact.stations === 0
        ? 'Dieser Knoten ist direkt erreichbar — dazwischen liegt nichts, was sich messen ließe.'
        : 'Ein Paket läuft den bekannten Weg ab und meldet jede Strecke. Kostet Sendezeit.';

  const start = async () => {
    setBusy(true);
    try {
      await apiPost('/nodes/traces', { public_key: contact.public_key });
      setError(null);
      onStarted();
    } catch (cause) {
      setError(describeError(cause as ApiError));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div>
      <div className="flex flex-wrap items-center gap-3 px-4 py-3">
        <button
          type="button"
          onClick={start}
          disabled={busy || !traceable}
          className="rounded-md border border-mesh-accent px-3 py-1.5 text-sm text-mesh-text hover:bg-mesh-raised disabled:border-mesh-border disabled:text-mesh-faint focus-visible:outline focus-visible:outline-2 focus-visible:outline-mesh-accent"
        >
          {busy ? 'wird gesendet …' : 'Weg messen'}
        </button>
        <span className="text-xs text-mesh-faint">{why}</span>
      </div>

      {error !== null && (
        <p className="px-4 pb-3 text-xs text-mesh-bad" role="alert">
          {error}
        </p>
      )}

      {traces === null ? (
        <Loading what="Die Messungen" />
      ) : traces.length === 0 ? (
        <Empty>Noch kein Weg gemessen.</Empty>
      ) : (
        <ul className="divide-y divide-mesh-border border-t border-mesh-border text-sm">
          {traces.map((trace) => (
            <li key={trace.id} className="px-4 py-3">
              <div className="flex flex-wrap items-baseline justify-between gap-x-4 gap-y-1">
                <span className="text-mesh-text">
                  {trace.answered_at === null ? (
                    <span className="text-mesh-warn">keine Antwort</span>
                  ) : (
                    <>
                      {trace.hops.length} {trace.hops.length === 1 ? 'Station' : 'Stationen'}
                      {trace.final_snr !== null && (
                        <>
                          <span className="mx-2 text-mesh-faint">letzte Strecke</span>
                          <SignalValue snr={trace.final_snr} />
                        </>
                      )}
                    </>
                  )}
                </span>
                <span
                  className="tabular shrink-0 text-xs text-mesh-muted"
                  title={exactTime(trace.asked_at)}
                >
                  {relativeTime(trace.asked_at, new Date(now))}
                </span>
              </div>

              {trace.hops.length > 0 && (
                <ol className="mt-1.5 flex flex-wrap items-center gap-x-2 gap-y-1 text-xs">
                  {trace.hops.map((hop, index) => (
                    <li key={`${trace.id}-${index}`} className="flex items-center gap-1.5">
                      {index > 0 && <span className="text-mesh-faint">→</span>}
                      {/* A one-byte prefix is not an identity: several nodes
                          may start with it. Shown as what it is. */}
                      <span className="tabular text-mesh-muted">{hop.key_prefix}</span>
                      {hop.snr !== null && <SignalValue snr={hop.snr} />}
                    </li>
                  ))}
                </ol>
              )}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

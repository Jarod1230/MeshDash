import { useEffect } from 'react';
import { Link } from 'react-router-dom';
import { exactTime, relativeTime } from '../lib/time';
import { SignalValue } from '../ui/Signal';
import type { PairFacts } from './pair';

/**
 * What is known about one connection, without leaving the map.
 *
 * The line is a summary; this is what it summarises. Two things it can say
 * that the line cannot:
 *
 * **Which way round.** Hearing is not symmetric — different antenna heights,
 * different transmit power, a hill on one side. "A hears B, and B has never
 * been heard by A" is usually the finding somebody is after, and a single line
 * between two dots cannot show it.
 *
 * **How it is known.** Reached directly, measured by a trace, or overheard in
 * passing are three claims of three different strengths, and the panel keeps
 * them apart rather than averaging them into one.
 */
export function LinkPanel({
  facts,
  now,
  onClose,
}: {
  readonly facts: PairFacts;
  readonly now: number;
  readonly onClose: () => void;
}) {
  useEffect(() => {
    const close = (event: KeyboardEvent) => {
      if (event.key === 'Escape') onClose();
    };
    window.addEventListener('keydown', close);

    return () => window.removeEventListener('keydown', close);
  }, [onClose]);

  const name = (key: string) =>
    key === facts.a.key ? facts.a.name : key === facts.b.key ? facts.b.name : key.slice(0, 6);
  const weak = facts.overheard.some((one) => one.width === 1);
  const oneWay = facts.overheard.length === 1;

  return (
    <aside
      className="pointer-events-auto absolute inset-x-0 bottom-0 z-10 max-h-[70dvh] overflow-y-auto border-t border-mesh-border bg-mesh-surface/95 p-4 backdrop-blur sm:inset-x-auto sm:top-16 sm:bottom-auto sm:left-4 sm:max-h-[calc(100dvh-6rem)] sm:w-80 sm:rounded-lg sm:border"
      aria-label={`Verbindung ${facts.a.name} und ${facts.b.name}`}
    >
      <div className="flex items-start justify-between gap-3">
        <h2 className="min-w-0 text-lg text-mesh-text">
          <span className="block truncate">{facts.a.name}</span>
          <span className="block truncate text-mesh-muted">
            <span className="text-mesh-faint">und</span> {facts.b.name}
          </span>
        </h2>
        <button
          type="button"
          onClick={onClose}
          aria-label="Schließen"
          className="shrink-0 rounded-md border border-mesh-border px-2 py-0.5 text-sm text-mesh-muted hover:text-mesh-text focus-visible:outline focus-visible:outline-2 focus-visible:outline-mesh-accent"
        >
          ✕
        </button>
      </div>

      <dl className="mt-4 space-y-3 text-sm">
        {facts.direct && (
          <Fact term="Erreichbarkeit">
            Dieser Node erreicht die Gegenstelle ohne Station dazwischen.
            <span className="text-mesh-faint"> Wie gut, sagt das nicht.</span>
          </Fact>
        )}

        {facts.overheard.map((one) => (
          <Fact key={`${one.talker}>${one.listener}`} term="Mitgehört">
            <span className="text-mesh-text">
              {name(one.talker)} <span className="text-mesh-faint">wurde gehört von</span>{' '}
              {name(one.listener)}
            </span>
            <p className="tabular mt-0.5 text-xs text-mesh-muted">
              {one.heard} {one.heard === 1 ? 'Paket' : 'Pakete'} ·{' '}
              <span title={exactTime(new Date(one.first).toISOString())}>
                seit {relativeTime(new Date(one.first).toISOString(), new Date(now))}
              </span>{' '}
              · zuletzt {relativeTime(new Date(one.last).toISOString(), new Date(now))}
            </p>
          </Fact>
        ))}

        {facts.measured.length > 0 && (
          <Fact term="Gemessen">
            <ul className="space-y-1">
              {facts.measured.slice(0, 4).map((one, index) => (
                <li key={index} className="flex items-baseline gap-2">
                  {one.snr === null ? (
                    <span className="text-xs text-mesh-faint">ohne Wert</span>
                  ) : (
                    <SignalValue snr={one.snr} />
                  )}
                  <span className="text-xs text-mesh-muted">
                    {relativeTime(new Date(one.at).toISOString(), new Date(now))}
                  </span>
                </li>
              ))}
            </ul>
          </Fact>
        )}

        {!facts.direct && facts.overheard.length === 0 && facts.measured.length === 0 && (
          <Fact term="Belege">
            <span className="text-mesh-warn">keine mehr</span>
            <p className="mt-0.5 text-xs text-mesh-muted">
              Diese Verbindung war einmal belegt und ist es jetzt nicht mehr — vermutlich ist der
              Beleg aus dem Verlauf gefallen.
            </p>
          </Fact>
        )}
      </dl>

      {oneWay && (
        <p className="mt-4 text-xs text-mesh-faint">
          Nur eine Richtung ist belegt. Das heißt nicht, dass die andere nicht funktioniert — nur,
          dass hier kein Paket ankam, das sie gezeigt hätte. Bei LoRa sind Verbindungen oft
          unsymmetrisch.
        </p>
      )}

      {weak && (
        <p className="mt-2 text-xs text-mesh-faint">
          Mitgehörtes stützt sich hier auf Ein-Byte-Präfixe: 256 Möglichkeiten, also kann ein
          Eintrag auch einen anderen Knoten meinen.
        </p>
      )}

      <div className="mt-5 flex flex-wrap gap-2">
        <Step to={`/knoten/${facts.a.key}`}>{facts.a.name}</Step>
        <Step to={`/knoten/${facts.b.key}`}>{facts.b.name}</Step>
      </div>
    </aside>
  );
}

function Fact({ term, children }: { readonly term: string; readonly children: React.ReactNode }) {
  return (
    <div>
      <dt className="text-xs uppercase tracking-wider text-mesh-faint">{term}</dt>
      <dd className="mt-0.5 text-mesh-text">{children}</dd>
    </div>
  );
}

function Step({ to, children }: { readonly to: string; readonly children: string }) {
  return (
    <Link
      to={to}
      className="rounded-md border border-mesh-accent px-3 py-1.5 text-sm text-mesh-text hover:bg-mesh-raised focus-visible:outline focus-visible:outline-2 focus-visible:outline-mesh-accent"
    >
      {children}
    </Link>
  );
}

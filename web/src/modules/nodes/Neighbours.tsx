import { Link } from 'react-router-dom';
import { Empty } from '../../ui/States';
import { SignalValue } from '../../ui/Signal';
import { relativeTime } from '../../lib/time';
import type { Direction, Neighbour } from '../../lib/neighbours';

/**
 * Who this node hears, and who hears it.
 *
 * Two sources side by side, because they answer different questions and cost
 * different things:
 *
 * **Mitgehört** accumulates from listening — nobody transmits for it. Its
 * weakness is the naming: a station is one to three bytes of a key, and at one
 * byte several nodes fit.
 *
 * **Gemessen** comes from a traceroute, the only measurement of how two other
 * stations hear each other. It costs airtime and happens when somebody asks.
 *
 * They are not merged into one score. "Twelve packets showed it" and "it
 * measured −4 dB" are different statements, and a reader deciding where to put
 * an antenna needs both, not their average.
 */
export function Neighbours({
  neighbours,
  now,
}: {
  readonly neighbours: readonly Neighbour[];
  readonly now: number;
}) {
  if (neighbours.length === 0) {
    return (
      <Empty>
        Zu diesem Knoten ist keine Hörbeziehung belegt. Sie entsteht von selbst, sobald ein Paket
        eintrifft, das über ihn lief — oder durch eine Wegmessung, die Sendezeit kostet.
      </Empty>
    );
  }

  const weak = neighbours.some(
    (one) => (one.hears?.width ?? 3) === 1 || (one.heardBy?.width ?? 3) === 1,
  );

  return (
    <>
      <ul className="divide-y divide-mesh-border text-sm">
        {neighbours.map((neighbour) => (
          <li key={neighbour.key} className="px-4 py-3">
            <div className="flex flex-wrap items-baseline justify-between gap-x-4 gap-y-1">
              <Link
                to={`/knoten/${neighbour.key}`}
                className="text-mesh-text hover:text-mesh-accent hover:underline focus-visible:outline focus-visible:outline-2 focus-visible:outline-mesh-accent"
              >
                {neighbour.name}
              </Link>
              {neighbour.measured.length > 0 && (
                <span className="flex shrink-0 items-baseline gap-2">
                  {neighbour.measured[0]?.snr === null ? (
                    <span className="text-xs text-mesh-faint">gemessen, ohne Wert</span>
                  ) : (
                    <>
                      <SignalValue snr={neighbour.measured[0]!.snr} />
                      <span className="text-xs text-mesh-faint">gemessen</span>
                    </>
                  )}
                </span>
              )}
            </div>

            <ul className="mt-1 space-y-0.5 text-xs">
              <Way
                label="hört"
                other={neighbour.name}
                direction={neighbour.hears}
                now={now}
              />
              <Way
                label="wird gehört von"
                other={neighbour.name}
                direction={neighbour.heardBy}
                now={now}
              />
            </ul>
          </li>
        ))}
      </ul>

      {weak && (
        <p className="border-t border-mesh-border px-4 py-2.5 text-xs text-mesh-faint">
          Ein Teil davon stützt sich auf Ein-Byte-Präfixe: 256 Möglichkeiten, also kann eine
          Beziehung auch einen anderen Knoten meinen. Wie breit ein Absender seine Stationen
          schreibt, entscheidet er selbst.
        </p>
      )}
    </>
  );
}

/**
 * One direction, or the honest absence of it.
 *
 * A missing direction is written out rather than left off. Nothing heard is a
 * gap in what was observed, not a statement that the link does not carry —
 * and leaving the line away would let a reader take it for the latter.
 */
function Way({
  label,
  other,
  direction,
  now,
}: {
  readonly label: string;
  readonly other: string;
  readonly direction: Direction | null;
  readonly now: number;
}) {
  if (direction === null) {
    return (
      <li className="text-mesh-faint">
        {label} {other}: <span className="text-mesh-warn">nichts beobachtet</span>
      </li>
    );
  }

  return (
    <li className="text-mesh-muted">
      {label} {other}
      <span className="tabular text-mesh-faint">
        {' · '}
        {direction.heard} {direction.heard === 1 ? 'Paket' : 'Pakete'} · zuletzt{' '}
        {relativeTime(new Date(direction.last).toISOString(), new Date(now))}
      </span>
    </li>
  );
}

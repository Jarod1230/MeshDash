import { useEffect } from 'react';
import { Link } from 'react-router-dom';
import { exactTime, relativeTime } from '../lib/time';
import { heard, type GroundNode } from './projection';

/**
 * What one node is, without leaving the map.
 *
 * The most frequent move an operator makes is "what is that dot" — see
 * ADR-0011, layer 2. It costs a click and no page change: the map stays where
 * it was, the node stays visible and marked, and the way on to everything
 * about it is one step further.
 *
 * Deliberately says only what the contact list already carries. Anything that
 * would need its own request belongs on the full page, which is one click
 * away — a panel that loads for a second on every dot is worse than a panel
 * that says less.
 */
export function NodePanel({
  node,
  now,
  onClose,
}: {
  readonly node: GroundNode;
  readonly now: number;
  readonly onClose: () => void;
}) {
  const state = heard(node.lastSeen, now);

  // Escape closes the panel, the same key that closes the shutter. Only one
  // of the two is ever open at a time, so they cannot fight over it.
  useEffect(() => {
    const close = (event: KeyboardEvent) => {
      if (event.key === 'Escape') onClose();
    };
    window.addEventListener('keydown', close);

    return () => window.removeEventListener('keydown', close);
  }, [onClose]);

  return (
    <aside
      // A drawer from the bottom on a narrow screen, a card at the side on a
      // wide one — the layers stay the same, only their direction changes.
      // Height follows the content: a full-height column of empty space next
      // to the map claims more of it than the panel needs.
      className="pointer-events-auto absolute inset-x-0 bottom-0 z-10 max-h-[70dvh] overflow-y-auto border-t border-mesh-border bg-mesh-surface/95 p-4 backdrop-blur sm:inset-x-auto sm:top-16 sm:bottom-auto sm:left-4 sm:max-h-[calc(100dvh-6rem)] sm:w-80 sm:rounded-lg sm:border"
      aria-label={`Knoten ${node.name}`}
    >
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <h2 className="truncate text-lg text-mesh-text">{node.name}</h2>
          <p className="tabular mt-0.5 truncate text-xs text-mesh-faint" title={node.key}>
            {node.key.slice(0, 16)}…
          </p>
        </div>
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
        <Fact term="Zuletzt gehört">
          <span className={state === 'lange' ? 'text-mesh-warn' : 'text-mesh-text'}>
            {relativeTime(new Date(node.lastSeen).toISOString(), new Date(now))}
          </span>
          <span className="text-mesh-faint"> · {exactTime(new Date(node.lastSeen).toISOString())}</span>
        </Fact>

        <Fact term="Weg">
          {node.own ? (
            'dieser Node'
          ) : node.stations === null ? (
            // Not the same as "directly reachable": one is a statement about
            // the mesh, the other about a gap in what this node knows.
            <span className="text-mesh-warn">kein Weg bekannt</span>
          ) : node.stations === 0 ? (
            'direkt erreichbar'
          ) : (
            `über ${node.stations} ${node.stations === 1 ? 'Station' : 'Stationen'}`
          )}
        </Fact>

        <Fact term="Position">
          {node.latitude === null || node.longitude === null ? (
            'meldet keine'
          ) : (
            <>
              <span className="tabular">
                {node.latitude.toFixed(5)}, {node.longitude.toFixed(5)}
              </span>
              <span className="text-mesh-faint">
                {node.source === 'telemetry' ? ' · aus Telemetrie' : ' · aus dem Advert'}
              </span>
            </>
          )}
        </Fact>
      </dl>

      <Link
        to={`/knoten/${node.key}`}
        className="mt-5 inline-block rounded-md border border-mesh-accent px-3 py-1.5 text-sm text-mesh-text hover:bg-mesh-raised focus-visible:outline focus-visible:outline-2 focus-visible:outline-mesh-accent"
      >
        Alles zu diesem Knoten
      </Link>
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

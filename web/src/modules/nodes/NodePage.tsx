import { Link, useParams } from 'react-router-dom';
import { SignalBars, SignalValue } from '../../ui/Signal';
import { Empty, Failed, Loading } from '../../ui/States';
import { exactTime, relativeTime } from '../../lib/time';
import { useNow } from '../../lib/useNow';
import { usePagedResource } from '../../lib/usePagedResource';
import { useResource } from '../../lib/useResource';
import { More } from '../../ui/More';
import { PositionForm } from './PositionForm';
import { PresenceBand, type Presence } from '../../ui/PresenceBand';
import { RangePicker } from '../../ui/RangePicker';
import { useTimeRange } from '../../lib/timeRange';
import {
  describeRoute,
  shortKey,
  type KnownContact,
  type RouteChange,
  type Sighting,
} from './types';
import type { ConversationMessage } from '../messages/types';

/**
 * Everything known about one node, in one place.
 *
 * # Why this page reads four modules
 *
 * Identity and route belong to `nodes`, the message thread to `messages`, the
 * readings it reported to `telemetry`. The module rules forbid one module from
 * reading another's tables — they say nothing about a browser calling several
 * public APIs, which is what a client does. See ADR-0010, where the same
 * reasoning kept the map out of a module of its own.
 */
interface NeighbourSample {
  readonly public_key: string;
  readonly at: string;
  readonly channel: number;
  readonly type_code: number;
  readonly value: number | null;
  readonly axes: readonly [number, number, number] | null;
  readonly position: readonly [number, number, number] | null;
}

/** How many sightings one page of the history holds. */
const SIGHTINGS_PAGE = 25;

/** How many route changes one page holds. Rarer than sightings, so fewer. */
const ROUTE_CHANGES_PAGE = 15;

export function NodePage() {
  const { key = '' } = useParams();
  const now = useNow();

  const contacts = useResource<KnownContact[]>('/nodes/contacts');
  const sightings = usePagedResource<Sighting>(`/nodes/adverts?node=${key}`, SIGHTINGS_PAGE);
  const readings = useResource<NeighbourSample[]>(`/telemetry/neighbours?node=${key}&limit=50`);
  const range = useTimeRange('7d');
  const presence = useResource<Presence>(`/nodes/presence?node=${key}${range.query}`);
  const routeChanges = usePagedResource<RouteChange>(
    `/nodes/route-changes?node=${key}`,
    ROUTE_CHANGES_PAGE,
  );
  const thread = useResource<ConversationMessage[]>(
    `/messages/conversation?with=${key.slice(0, 12)}&limit=50`,
  );

  if (contacts.error !== null && contacts.data === null) {
    return (
      <div className="rounded-lg border border-mesh-border bg-mesh-surface">
        <Failed error={contacts.error} onRetry={contacts.reload} />
      </div>
    );
  }

  if (contacts.data === null) {
    return (
      <div className="rounded-lg border border-mesh-border bg-mesh-surface">
        <Loading what="Der Knoten" />
      </div>
    );
  }

  const contact = contacts.data.find((candidate) => candidate.public_key === key);

  if (contact === undefined) {
    return (
      <div className="rounded-lg border border-mesh-border bg-mesh-surface px-4 py-6">
        <p className="text-mesh-text">Dieser Knoten ist nicht bekannt.</p>
        <p className="mt-1 text-sm text-mesh-muted">
          Vielleicht wurde er aus der Kontaktliste des Node verdrängt.
        </p>
        <Link to="/knoten" className="mt-3 inline-block text-sm text-mesh-accent hover:underline">
          ← Alle Knoten
        </Link>
      </div>
    );
  }

  const silent = (now - new Date(contact.last_seen).getTime()) / 1000 > 86_400;

  return (
    <div className="space-y-4">
      <Link to="/knoten" className="inline-block text-sm text-mesh-accent hover:underline">
        ← Alle Knoten
      </Link>

      <section className="rounded-lg border border-mesh-border bg-mesh-surface p-5">
        <div className="flex flex-wrap items-baseline gap-x-4 gap-y-2">
          <h2 className="text-xl text-mesh-text">{contact.name}</h2>
          {silent && (
            <span className="text-xs uppercase tracking-wider text-mesh-warn">
              schweigt seit über einem Tag
            </span>
          )}
        </div>
        <p className="tabular mt-1 text-xs break-all text-mesh-faint">{contact.public_key}</p>

        <dl className="mt-4 flex flex-wrap gap-x-8 gap-y-3 text-sm">
          <Fact term="Weg" value={describeRoute(contact.stations)} />
          <Fact term="Zuletzt gehört" value={relativeTime(contact.last_seen, new Date(now))} />
          <Fact term="Bekannt seit" value={relativeTime(contact.first_seen, new Date(now))} />
          <Fact
            term="Position"
            value={
              contact.latitude === null || contact.longitude === null
                ? 'keine'
                : `${contact.latitude.toFixed(5)}, ${contact.longitude.toFixed(5)}${
                    contact.position_source === 'manual' ? ' (gesetzt)' : ''
                  }`
            }
          />
          <Fact term="Typ" value={String(contact.contact_type)} />
        </dl>

        <div className="mt-5 border-t border-mesh-border pt-4">
          <PositionForm contact={contact} onSaved={contacts.reload} />
        </div>
      </section>

      <section className="rounded-lg border border-mesh-border bg-mesh-surface">
        <header className="flex flex-wrap items-baseline justify-between gap-3 border-b border-mesh-border px-4 py-2.5">
          <div className="flex flex-wrap items-baseline gap-3">
            <h2 className="text-sm text-mesh-text">Erreichbarkeit</h2>
            <span className="text-xs text-mesh-faint">
              wie oft dieser Knoten je Abschnitt zu hören war
            </span>
          </div>
          <RangePicker range={range} label="Zeitraum der Erreichbarkeit" />
        </header>
        <div className="px-4 py-3">
          {presence.error !== null && presence.data === null ? (
            <p className="text-xs text-mesh-faint">Die Erreichbarkeit konnte nicht geladen werden.</p>
          ) : presence.data === null ? (
            <Loading what="Die Erreichbarkeit" />
          ) : (
            <PresenceBand presence={presence.data} />
          )}
        </div>
      </section>

      <Panel title="Wegwechsel" hint="wenn das Mesh diesen Knoten neu geroutet hat">
        {routeChanges.items === null ? (
          <Loading what="Die Wegwechsel" />
        ) : routeChanges.items.length === 0 ? (
          <Empty>
            Der Weg zu diesem Knoten hat sich nicht geändert, seit MeshDash ihn kennt. Aufgezeichnet
            wird erst ab dem zweiten bekannten Weg — der erste ist der Anfang der Geschichte, nicht
            ein Schritt darin.
          </Empty>
        ) : (
          <>
            <ul className="divide-y divide-mesh-border text-sm">
              {routeChanges.items.map((change) => (
                <li key={change.id} className="px-4 py-2.5">
                  <div className="flex flex-wrap items-baseline justify-between gap-x-4 gap-y-1">
                    <span className="text-mesh-text">
                      <span className="text-mesh-muted">{describeRoute(change.previous_stations)}</span>
                      <span className="mx-2 text-mesh-faint">→</span>
                      {describeRoute(change.stations)}
                    </span>
                    <span
                      className="tabular shrink-0 text-xs text-mesh-muted"
                      title={exactTime(change.changed_at)}
                    >
                      {relativeTime(change.changed_at, new Date(now))}
                    </span>
                  </div>
                  {/* The hop bytes themselves, for whoever compares two routes
                      station by station. Hex, because that is what they are. */}
                  <p className="tabular mt-0.5 truncate text-xs text-mesh-faint">
                    {change.previous_path === null || change.previous_path === ''
                      ? '—'
                      : change.previous_path}
                    {' → '}
                    {change.path === null || change.path === '' ? '—' : change.path}
                  </p>
                </li>
              ))}
            </ul>
            {routeChanges.hasMore && (
              <More
                onClick={routeChanges.loadMore}
                loading={routeChanges.loadingMore}
                what="Wegwechsel"
              />
            )}
          </>
        )}
      </Panel>

      <Panel title="Sichtungen" hint="wann dieser Knoten zu hören war">
        {sightings.items === null ? (
          <Loading what="Die Sichtungen" />
        ) : sightings.items.length === 0 ? (
          <Empty>
            Kein Advert dieses Knotens aufgezeichnet. Er ist über die Kontaktliste des eigenen Node
            bekannt, hat sich aber seither nicht selbst gemeldet.
          </Empty>
        ) : (
          <>
            <ul className="divide-y divide-mesh-border text-sm">
              {sightings.items.map((sighting) => (
                <li
                  key={sighting.id}
                  className="flex items-baseline justify-between gap-4 px-4 py-2"
                >
                  <span className="text-mesh-muted">
                    {sighting.was_new ? 'erstmals gehört' : 'gehört'}
                  </span>
                  <span className="tabular text-mesh-text" title={exactTime(sighting.heard_at)}>
                    {relativeTime(sighting.heard_at, new Date(now))}
                  </span>
                </li>
              ))}
            </ul>
            {sightings.hasMore && (
              <More onClick={sightings.loadMore} loading={sightings.loadingMore} what="Sichtungen" />
            )}
          </>
        )}
      </Panel>

      <Panel title="Nachrichten" hint="Verlauf mit diesem Knoten">
        {thread.data === null ? (
          <Loading what="Der Verlauf" />
        ) : thread.data.length === 0 ? (
          <Empty>Mit diesem Knoten wurde noch nichts ausgetauscht.</Empty>
        ) : (
          <ul className="divide-y divide-mesh-border text-sm">
            {thread.data.slice(-8).map((message, index) => (
              <li key={`${message.at}-${index}`} className="px-4 py-2">
                <div className="flex items-baseline justify-between gap-4">
                  <p className="min-w-0 text-mesh-text">
                    {message.direction === 'sent' && (
                      <span className="mr-1.5 text-xs text-mesh-faint">Sie:</span>
                    )}
                    {message.text}
                  </p>
                  <span
                    className="tabular shrink-0 text-xs text-mesh-muted"
                    title={exactTime(message.at)}
                  >
                    {relativeTime(message.at, new Date(now))}
                  </span>
                </div>
                {message.direction === 'received' && (
                  <div className="mt-1 flex items-center gap-2 text-xs text-mesh-faint">
                    <SignalBars snr={message.snr} />
                    <SignalValue snr={message.snr} />
                  </div>
                )}
              </li>
            ))}
          </ul>
        )}
      </Panel>

      <Panel title="Gemeldete Messwerte" hint="was dieser Knoten über sich selbst sagt">
        {readings.data === null ? (
          <Loading what="Die Messwerte" />
        ) : readings.data.length === 0 ? (
          <Empty>
            Dieser Knoten wurde nicht nach Messwerten gefragt, oder hat nicht geantwortet. Gefragt
            wird nur, wenn <code className="text-mesh-accent">[modules.telemetry] neighbours</code>{' '}
            eingeschaltet ist.
          </Empty>
        ) : (
          <ul className="divide-y divide-mesh-border text-sm">
            {readings.data.slice(0, 12).map((sample) => (
              <li
                key={`${sample.at}-${sample.channel}-${sample.type_code}`}
                className="flex flex-wrap items-baseline gap-x-3 px-4 py-2"
              >
                <span className="text-mesh-muted">Typ {sample.type_code}</span>
                <span className="tabular text-mesh-text">{describeReading(sample)}</span>
                <span
                  className="tabular ml-auto text-xs text-mesh-faint"
                  title={exactTime(sample.at)}
                >
                  {relativeTime(sample.at, new Date(now))}
                </span>
              </li>
            ))}
          </ul>
        )}
      </Panel>
    </div>
  );
}

/** One reading in words, whichever shape it has. */
function describeReading(sample: NeighbourSample): string {
  if (sample.position !== null) {
    const [latitude, longitude, altitude] = sample.position;
    return `${latitude.toFixed(4)}°, ${longitude.toFixed(4)}°, ${altitude.toFixed(0)} m`;
  }
  if (sample.axes !== null) return sample.axes.map((axis) => axis.toFixed(2)).join(' / ');
  return sample.value === null ? '—' : sample.value.toFixed(2);
}

function Fact({ term, value }: { readonly term: string; readonly value: string }) {
  return (
    <div>
      <dt className="text-xs uppercase tracking-wider text-mesh-faint">{term}</dt>
      <dd className="tabular mt-0.5 text-mesh-text">{value}</dd>
    </div>
  );
}

function Panel({
  title,
  hint,
  children,
}: {
  readonly title: string;
  readonly hint: string;
  readonly children: React.ReactNode;
}) {
  return (
    <section className="rounded-lg border border-mesh-border bg-mesh-surface">
      <header className="flex flex-wrap items-baseline gap-3 border-b border-mesh-border px-4 py-2.5">
        <h2 className="text-sm text-mesh-text">{title}</h2>
        <span className="text-xs text-mesh-faint">{hint}</span>
      </header>
      {children}
    </section>
  );
}

/** The key shown in a list, short enough to read. */
export { shortKey };

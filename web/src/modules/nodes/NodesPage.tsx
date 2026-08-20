import { useState } from 'react';
import { Empty, Failed, Loading } from '../../ui/States';
import { Topology } from './Topology';
import { hopCount, shortKey, type KnownContact, type Sighting } from './types';
import { useLiveReload, type AppEvent } from '../../lib/events';
import { useNow } from '../../lib/useNow';
import { useResource } from '../../lib/useResource';
import { exactTime, relativeTime } from '../../lib/time';
import { isAdvert } from '../../lib/pushes';

/**
 * Who is out there, and who is still answering.
 *
 * Two views of the same set, because the two questions differ: the list
 * answers "what do I know about this node", the ring chart answers "how far
 * away is it and does it still reply".
 */
export function NodesPage() {
  const now = useNow();
  const contacts = useResource<KnownContact[]>('/nodes/contacts');
  const sightings = useResource<Sighting[]>('/nodes/adverts?limit=50');
  const [view, setView] = useState<'liste' | 'netz'>('liste');

  // An advert means someone was heard; both listings change.
  useLiveReload(
    (event: AppEvent) => event.type === 'push' && isAdvert(event.payload),
    () => {
      contacts.reload();
      sightings.reload();
    },
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
        <Loading what="Die Knotenliste" />
      </div>
    );
  }

  const heard = contacts.data.filter(
    (contact) => (now - new Date(contact.last_seen).getTime()) / 1000 < 3600,
  );

  return (
    <div className="space-y-4">
      <div className="flex flex-wrap items-end justify-between gap-4">
        <dl className="flex gap-6">
          <Figure label="bekannt" value={contacts.data.length} />
          <Figure label="in der letzten Stunde gehört" value={heard.length} accent />
        </dl>

        <div className="flex gap-1" role="tablist" aria-label="Darstellung">
          {(['liste', 'netz'] as const).map((option) => (
            <button
              key={option}
              type="button"
              role="tab"
              aria-selected={view === option}
              onClick={() => setView(option)}
              className={`rounded-md border px-3 py-1 text-sm capitalize focus-visible:outline focus-visible:outline-2 focus-visible:outline-mesh-accent ${
                view === option
                  ? 'border-mesh-accent text-mesh-text'
                  : 'border-mesh-border text-mesh-muted hover:text-mesh-text'
              }`}
            >
              {option}
            </button>
          ))}
        </div>
      </div>

      <section className="rounded-lg border border-mesh-border bg-mesh-surface">
        {view === 'netz' ? (
          <Topology contacts={contacts.data} now={now} />
        ) : contacts.data.length === 0 ? (
          <Empty>
            Der Node kennt noch keine Kontakte. Sie erscheinen, sobald er welche meldet oder ein
            Advert eintrifft.
          </Empty>
        ) : (
          <ContactTable contacts={contacts.data} now={now} />
        )}
      </section>

      <section className="rounded-lg border border-mesh-border bg-mesh-surface">
        <header className="flex items-baseline gap-3 border-b border-mesh-border px-4 py-2.5">
          <h2 className="text-sm text-mesh-text">Sichtungen</h2>
          <span className="text-xs text-mesh-faint">wer sich zuletzt gemeldet hat</span>
        </header>
        {sightings.data === null || sightings.data.length === 0 ? (
          <Empty>
            Noch nichts gehört. Jedes Advert, das eintrifft, wird hier festgehalten — auch von einem
            Knoten, zu dem es noch keinen Kontakt gibt.
          </Empty>
        ) : (
          <ul className="divide-y divide-mesh-border text-sm">
            {sightings.data.slice(0, 15).map((sighting) => {
              const contact = contacts.data?.find(
                (candidate) => candidate.public_key === sighting.public_key,
              );
              return (
                <li
                  key={`${sighting.public_key}-${sighting.heard_at}`}
                  className="flex items-baseline justify-between gap-4 px-4 py-2"
                >
                  <span className="min-w-0 truncate">
                    <span className="text-mesh-text">
                      {contact?.name ?? 'Unbekannter Knoten'}
                    </span>
                    <span className="tabular ml-2 text-xs text-mesh-faint">
                      {shortKey(sighting.public_key)}
                    </span>
                    {sighting.was_new && (
                      <span className="ml-2 text-xs text-mesh-accent">neu</span>
                    )}
                  </span>
                  <span
                    className="tabular shrink-0 text-mesh-muted"
                    title={exactTime(sighting.heard_at)}
                  >
                    {relativeTime(sighting.heard_at, new Date(now))}
                  </span>
                </li>
              );
            })}
          </ul>
        )}
      </section>
    </div>
  );
}

function Figure({
  label,
  value,
  accent = false,
}: {
  readonly label: string;
  readonly value: number;
  readonly accent?: boolean;
}) {
  return (
    <div>
      <dd className={`tabular text-3xl leading-none ${accent ? 'text-mesh-accent' : 'text-mesh-text'}`}>
        {value}
      </dd>
      <dt className="mt-1 text-xs text-mesh-muted">{label}</dt>
    </div>
  );
}

function ContactTable({
  contacts,
  now,
}: {
  readonly contacts: readonly KnownContact[];
  readonly now: number;
}) {
  return (
    <div className="overflow-x-auto">
      <table className="w-full text-sm">
        <thead>
          <tr className="border-b border-mesh-border text-left text-xs uppercase tracking-wider text-mesh-faint">
            <th className="px-4 py-2 font-normal">Name</th>
            <th className="hidden px-4 py-2 font-normal sm:table-cell">Schlüssel</th>
            <th className="px-4 py-2 font-normal">Weg</th>
            <th className="whitespace-nowrap px-4 py-2 text-right font-normal">Zuletzt gehört</th>
            <th className="hidden px-4 py-2 text-right font-normal md:table-cell">Bekannt seit</th>
          </tr>
        </thead>
        <tbody className="divide-y divide-mesh-border">
          {contacts.map((contact) => {
            const silent = (now - new Date(contact.last_seen).getTime()) / 1000 > 86_400;
            const hops = hopCount(contact.path);
            return (
              <tr key={contact.public_key} className={silent ? 'text-mesh-muted' : ''}>
                <td className="px-4 py-2 text-mesh-text">{contact.name}</td>
                <td className="tabular hidden px-4 py-2 text-xs text-mesh-faint sm:table-cell">
                  {shortKey(contact.public_key)}
                </td>
                <td className="px-4 py-2 text-mesh-muted">
                  {hops === 0 ? 'direkt' : hops === 1 ? '1 Station' : `${hops} Stationen`}
                </td>
                <td
                  className="tabular whitespace-nowrap px-4 py-2 text-right"
                  title={exactTime(contact.last_seen)}
                >
                  {relativeTime(contact.last_seen, new Date(now))}
                </td>
                <td
                  className="tabular hidden px-4 py-2 text-right text-mesh-faint md:table-cell"
                  title={exactTime(contact.first_seen)}
                >
                  {relativeTime(contact.first_seen, new Date(now))}
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}

import { useMemo, useState } from 'react';
import { useSearchParams } from 'react-router-dom';
import { Failed, Loading } from '../../ui/States';
import { NewDirectThread, ThreadList, ThreadView } from './Conversations';
import {
  channelThreads,
  conversationsForArea,
  findThread,
  matchingThreads,
  parseArea,
  type Area,
} from './threads';
import type { Channel, Conversation } from './types';
import { useLiveReload, type AppEvent } from '../../lib/events';
import { useNow } from '../../lib/useNow';
import { useResource } from '../../lib/useResource';
import { SearchBox } from '../../ui/SearchBox';
import { useDebounced } from '../../lib/useDebounced';
import { isMessageWaiting } from '../../lib/pushes';

/**
 * Direct messages and channel messages as two areas of threads.
 *
 * # Why not one unified list
 *
 * Decided 2026-09-06: Direkt and Kanäle stay separate. A channel message has
 * no sender the interface could name — the sending firmware writes the name
 * into the text — so a combined list would have a column that is empty for
 * half its rows. Each area shows threads that interleave sent and received
 * for that peer or channel; compose lives inside the open thread, not in a
 * global form at the top.
 *
 * Flat receive lists (`/messages/received`, `/messages/channel-received`) are
 * no longer shown here. The API keeps them for analysis; the UI regroups via
 * `/messages/conversations` and `/messages/conversation`.
 *
 * Selection lives in the address (`?bereich=`, `?faden=`, `?neu=`), so a link
 * opens the same view a click opens.
 */
export function MessagesPage() {
  const now = useNow();
  const [params, setParams] = useSearchParams();
  const area = parseArea(params.get('bereich'));
  const threadId = params.get('faden');
  const isNew = params.get('neu') === '1' && area === 'direkt';

  const [search, setSearch] = useState('');
  const query = useDebounced(search.trim());

  const conversations = useResource<Conversation[]>('/messages/conversations?limit=100');
  const channels = useResource<Channel[]>('/messages/channels');
  // Remounts the open thread when live events arrive; it reads its own
  // resources, and remounting is simpler than threading a reload through.
  const [reloadKey, setReloadKey] = useState(0);

  useLiveReload(
    (event: AppEvent) => event.type === 'push' && isMessageWaiting(event.payload),
    () => {
      conversations.reload();
      channels.reload();
      setReloadKey((value) => value + 1);
    },
  );

  const setArea = (next: Area) => {
    const nextParams = new URLSearchParams(params);
    if (next === 'direkt') nextParams.delete('bereich');
    else nextParams.set('bereich', next);
    nextParams.delete('faden');
    nextParams.delete('neu');
    setParams(nextParams, { replace: true });
  };

  const openThread = (conversation: Conversation) => {
    const nextParams = new URLSearchParams(params);
    nextParams.set('faden', conversation.id);
    nextParams.delete('neu');
    setParams(nextParams, { replace: true });
  };

  const openNew = () => {
    const nextParams = new URLSearchParams(params);
    nextParams.delete('faden');
    nextParams.set('neu', '1');
    setParams(nextParams, { replace: true });
  };

  const closeThread = () => {
    const nextParams = new URLSearchParams(params);
    nextParams.delete('faden');
    nextParams.delete('neu');
    setParams(nextParams, { replace: true });
  };

  const afterSent = (opened?: { partner: 'contact' | 'channel'; id: string }) => {
    conversations.reload();
    channels.reload();
    setReloadKey((value) => value + 1);
    if (opened !== undefined) {
      const nextParams = new URLSearchParams(params);
      nextParams.set('faden', opened.id);
      nextParams.delete('neu');
      if (opened.partner === 'channel') nextParams.set('bereich', 'kanaele');
      else nextParams.delete('bereich');
      setParams(nextParams, { replace: true });
    }
  };

  const areaThreads = useMemo(() => {
    if (conversations.data === null) return null;
    if (area === 'kanaele') {
      return channelThreads(conversations.data, channels.data ?? []);
    }
    return conversationsForArea(conversations.data, 'direkt');
  }, [conversations.data, channels.data, area]);

  const shown = useMemo(() => {
    if (areaThreads === null) return null;
    return matchingThreads(areaThreads, query);
  }, [areaThreads, query]);

  const selected =
    isNew || shown === null ? null : findThread(shown, threadId) ?? findThread(areaThreads ?? [], threadId);

  if (conversations.error !== null && conversations.data === null) {
    return (
      <div className="rounded-lg border border-mesh-border bg-mesh-surface">
        <Failed error={conversations.error} onRetry={conversations.reload} />
      </div>
    );
  }

  if (conversations.data === null || shown === null) {
    return (
      <div className="rounded-lg border border-mesh-border bg-mesh-surface">
        <Loading what="Die Nachrichten" />
      </div>
    );
  }

  const backLabel = area === 'direkt' ? 'Alle Direktnachrichten' : 'Alle Kanäle';

  return (
    <div className="space-y-4">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="flex gap-1" role="tablist" aria-label="Art der Nachrichten">
          {(
            [
              { id: 'direkt' as const, label: 'Direkt' },
              { id: 'kanaele' as const, label: 'Kanäle' },
            ] as const
          ).map((option) => (
            <button
              key={option.id}
              type="button"
              role="tab"
              aria-selected={area === option.id}
              onClick={() => setArea(option.id)}
              className={`rounded-md border px-3 py-1 text-sm focus-visible:outline focus-visible:outline-2 focus-visible:outline-mesh-accent ${
                area === option.id
                  ? 'border-mesh-accent text-mesh-text'
                  : 'border-mesh-border text-mesh-muted hover:text-mesh-text'
              }`}
            >
              {option.label}
            </button>
          ))}
        </div>

        {selected === null && !isNew && (
          <SearchBox
            value={search}
            onChange={setSearch}
            label="Fäden durchsuchen"
            placeholder={
              area === 'direkt' ? 'Name, Präfix oder Text' : 'Kanalname oder Text'
            }
          />
        )}
      </div>

      <section className="rounded-lg border border-mesh-border bg-mesh-surface">
        {isNew ? (
          <NewDirectThread key={reloadKey} onBack={closeThread} onSent={afterSent} />
        ) : selected !== null ? (
          <ThreadView
            key={`${reloadKey}-${selected.partner}-${selected.id}`}
            conversation={selected}
            now={now}
            onBack={closeThread}
            onSent={() => afterSent()}
            backLabel={backLabel}
          />
        ) : (
          <>
            {area === 'direkt' && (
              <div className="flex justify-end border-b border-mesh-border px-4 py-2">
                <button
                  type="button"
                  onClick={openNew}
                  className="text-sm text-mesh-accent hover:underline focus-visible:outline focus-visible:outline-2 focus-visible:outline-mesh-accent"
                >
                  Neue Direktnachricht
                </button>
              </div>
            )}
            <ThreadList
              threads={shown}
              now={now}
              onSelect={openThread}
              empty={
                area === 'direkt' ? (
                  <>
                    Noch keine Direktnachrichten. Sobald etwas hereinkommt oder Sie etwas senden,
                    steht es hier als Faden — Empfangenes und Gesendetes zusammen. Über „Neue
                    Direktnachricht“ schreiben Sie jemanden an, der noch nicht in der Liste steht.
                  </>
                ) : (
                  <>
                    Noch keine Kanalnachrichten, und der Node meldet keine Kanäle. Kanäle erscheinen
                    hier, sobald der Node welche kennt oder etwas über einen Kanal läuft.
                  </>
                )
              }
              noMatch={
                query === ''
                  ? null
                  : `Kein Faden passt zu „${query}".`
              }
            />
          </>
        )}
      </section>
    </div>
  );
}

import { useState } from 'react';
import { SignalBars, SignalValue } from '../../ui/Signal';
import { Empty, Failed, Loading } from '../../ui/States';
import { Conversations } from './Conversations';
import { SendForm } from './SendForm';
import type { Channel, ChannelMessage, Conversation, DirectMessage } from './types';
import { useLiveReload, type AppEvent } from '../../lib/events';
import { useNow } from '../../lib/useNow';
import { usePagedResource } from '../../lib/usePagedResource';
import { useResource } from '../../lib/useResource';
import { More } from '../../ui/More';
import { exactTime, relativeTime } from '../../lib/time';
import { isMessageWaiting } from '../../lib/pushes';

/** How many messages one page holds. Big enough that most sessions never page. */
const PAGE = 100;

/**
 * What came in over the air, and what goes out.
 *
 * Direct messages and channel messages are kept apart rather than merged into
 * one stream: a channel message has no sender the interface could name — the
 * sending firmware writes the name into the text — so a combined list would
 * have a column that is empty for half its rows.
 */
export function MessagesPage() {
  const now = useNow();
  const direct = usePagedResource<DirectMessage>('/messages/received', PAGE);
  const channel = usePagedResource<ChannelMessage>('/messages/channel-received', PAGE);
  const channels = useResource<Channel[]>('/messages/channels');
  const [tab, setTab] = useState<'gespräche' | 'direkt' | 'kanäle'>('gespräche');
  const [openConversation, setOpenConversation] = useState<Conversation | null>(null);
  // Remounts the conversation view when live events arrive; it reads its own
  // resources, and remounting is simpler than threading a reload through two
  // levels of component.
  const [reloadKey, setReloadKey] = useState(0);

  // The node rings the bell; the backend fetches, then we reload.
  useLiveReload(
    (event: AppEvent) => event.type === 'push' && isMessageWaiting(event.payload),
    () => {
      direct.reload();
      channel.reload();
      setReloadKey((value) => value + 1);
    },
  );

  const reloadAll = () => {
    direct.reload();
    channel.reload();
  };

  if (direct.error !== null && direct.items === null) {
    return (
      <div className="rounded-lg border border-mesh-border bg-mesh-surface">
        <Failed error={direct.error} onRetry={direct.reload} />
      </div>
    );
  }

  return (
    <div className="space-y-4">
      <section className="rounded-lg border border-mesh-border bg-mesh-surface">
        <header className="border-b border-mesh-border px-4 py-2.5">
          <h2 className="text-sm text-mesh-text">Senden</h2>
        </header>
        <SendForm channels={channels.data ?? []} onSent={reloadAll} />
      </section>

      <div className="flex gap-1" role="tablist" aria-label="Art der Nachrichten">
        {(['gespräche', 'direkt', 'kanäle'] as const).map((option) => (
          <button
            key={option}
            type="button"
            role="tab"
            aria-selected={tab === option}
            onClick={() => setTab(option)}
            className={`rounded-md border px-3 py-1 text-sm capitalize focus-visible:outline focus-visible:outline-2 focus-visible:outline-mesh-accent ${
              tab === option
                ? 'border-mesh-accent text-mesh-text'
                : 'border-mesh-border text-mesh-muted hover:text-mesh-text'
            }`}
          >
            {option}
          </button>
        ))}
      </div>

      <section className="rounded-lg border border-mesh-border bg-mesh-surface">
        {tab === 'gespräche' ? (
          <Conversations
            key={reloadKey}
            now={now}
            selected={openConversation}
            onSelect={setOpenConversation}
          />
        ) : tab === 'direkt' ? (
          direct.items === null ? (
            <Loading what="Die Nachrichten" />
          ) : direct.items.length === 0 ? (
            <Empty>
              Noch keine Direktnachricht empfangen. Was hereinkommt, bleibt hier stehen — auch
              nachdem der Node seine eigene Warteschlange geleert hat.
            </Empty>
          ) : (
            <>
              <ul className="divide-y divide-mesh-border">
                {direct.items.map((message) => (
                <li key={message.id} className="px-4 py-3">
                  <div className="flex items-baseline justify-between gap-4">
                    {/* Foreign text, rendered as text and never as markup. */}
                    <p className="min-w-0 text-mesh-text">{message.text}</p>
                    <span
                      className="tabular shrink-0 text-xs text-mesh-muted"
                      title={exactTime(message.received_at)}
                    >
                      {relativeTime(message.received_at, new Date(now))}
                    </span>
                  </div>
                  <div className="mt-1 flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-mesh-faint">
                    <SignalBars snr={message.snr} />
                    <SignalValue snr={message.snr} />
                    <Sender message={message} />
                    <span>
                      {message.path_len === null
                        ? 'direkt empfangen'
                        : `über ${message.path_len} ${message.path_len === 1 ? 'Station' : 'Stationen'}`}
                    </span>
                  </div>
                  </li>
                ))}
              </ul>
              {direct.hasMore && (
                <More onClick={direct.loadMore} loading={direct.loadingMore} what="Nachrichten" />
              )}
            </>
          )
        ) : channel.items === null ? (
          <Loading what="Die Kanalnachrichten" />
        ) : channel.items.length === 0 ? (
          <Empty>
            In den Kanälen war es bisher still. Kanalnachrichten kommen über dieselbe Warteschlange
            wie Direktnachrichten herein.
          </Empty>
        ) : (
          <>
            <ul className="divide-y divide-mesh-border">
              {channel.items.map((message) => (
              <li key={message.id} className="px-4 py-3">
                <div className="flex items-baseline justify-between gap-4">
                  <p className="min-w-0 text-mesh-text">{message.text}</p>
                  <span
                    className="tabular shrink-0 text-xs text-mesh-muted"
                    title={exactTime(message.received_at)}
                  >
                    {relativeTime(message.received_at, new Date(now))}
                  </span>
                </div>
                <div className="mt-1 flex items-center gap-3 text-xs text-mesh-faint">
                  <SignalBars snr={message.snr} />
                  <SignalValue snr={message.snr} />
                  <span>
                    {channelName(channels.data, message.channel_index)}
                  </span>
                  {/* No sender: the sending firmware puts the name into the
                      text itself, so there is nothing here to attribute. */}
                  <span>Absender steht im Text</span>
                </div>
                </li>
              ))}
            </ul>
            {channel.hasMore && (
              <More onClick={channel.loadMore} loading={channel.loadingMore} what="Kanalnachrichten" />
            )}
          </>
        )}
      </section>
    </div>
  );
}

/**
 * Who sent a message, as far as that can be said.
 *
 * Six bytes of a key are not an identity: two contacts can share them. Where
 * that happens no name is shown and the interface says why — a guess presented
 * as fact is worse than a hex prefix, especially where messages carry
 * instructions.
 */
function Sender({ message }: { readonly message: DirectMessage }) {
  if (message.sender_name !== null) {
    return (
      <span>
        von <span className="text-mesh-muted">{message.sender_name}</span>
        <span className="tabular ml-1.5 text-mesh-faint">{message.sender_prefix}</span>
      </span>
    );
  }

  if (message.sender_candidates > 1) {
    return (
      <span
        className="tabular"
        title={`${message.sender_candidates} bekannte Knoten teilen sich dieses Schlüsselpräfix`}
      >
        von {message.sender_prefix} — mehrdeutig
      </span>
    );
  }

  return <span className="tabular">von {message.sender_prefix}</span>;
}

function channelName(channels: readonly Channel[] | null, index: number): string {
  const found = channels?.find((channel) => channel.channel_index === index);
  return found?.name !== undefined && found.name !== '' ? found.name : `Kanal ${index}`;
}

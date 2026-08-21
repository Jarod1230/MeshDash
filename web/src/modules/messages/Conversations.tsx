import { Link } from 'react-router-dom';
import { SignalBars } from '../../ui/Signal';
import { Empty, Failed, Loading } from '../../ui/States';
import { exactTime, relativeTime } from '../../lib/time';
import { useResource } from '../../lib/useResource';
import { conversationTitle, type Conversation, type ConversationMessage } from './types';

/**
 * Who has been talked to, and what was said.
 *
 * # Why one thread and not two lists
 *
 * Received and sent are stored apart and were shown apart, which cannot show
 * the one thing a conversation is: that an answer followed a question. Here
 * they are interleaved by time.
 *
 * A partner that was only written to appears as well — someone you have
 * messaged is a conversation before they answer.
 */
export function Conversations({
  now,
  selected,
  onSelect,
}: {
  readonly now: number;
  readonly selected: Conversation | null;
  readonly onSelect: (conversation: Conversation | null) => void;
}) {
  const conversations = useResource<Conversation[]>('/messages/conversations?limit=100');

  if (conversations.error !== null && conversations.data === null) {
    return <Failed error={conversations.error} onRetry={conversations.reload} />;
  }

  if (conversations.data === null) return <Loading what="Die Gespräche" />;

  if (conversations.data.length === 0) {
    return (
      <Empty>
        Noch keine Gespräche. Sobald etwas hereinkommt oder Sie etwas senden, steht es hier —
        Empfangenes und Gesendetes im selben Faden.
      </Empty>
    );
  }

  if (selected !== null) {
    return <Thread conversation={selected} now={now} onBack={() => onSelect(null)} />;
  }

  return (
    <ul className="divide-y divide-mesh-border">
      {conversations.data.map((conversation) => (
        <li key={`${conversation.partner}-${conversation.id}`}>
          <button
            type="button"
            onClick={() => onSelect(conversation)}
            className="flex w-full items-baseline gap-3 px-4 py-3 text-left hover:bg-mesh-raised focus-visible:outline focus-visible:outline-2 focus-visible:outline-mesh-accent"
          >
            <span className="min-w-0 flex-1">
              <span className="flex items-baseline gap-2">
                <span className="truncate text-mesh-text">{conversationTitle(conversation)}</span>
                <span className="shrink-0 text-xs text-mesh-faint">
                  {conversation.partner === 'channel' ? 'Kanal' : 'Kontakt'}
                </span>
              </span>
              <span className="mt-0.5 flex items-baseline gap-1.5 text-sm text-mesh-muted">
                {conversation.last_direction === 'sent' && (
                  <span className="text-xs text-mesh-faint">Sie:</span>
                )}
                <span className="truncate">{conversation.last_text}</span>
              </span>
            </span>
            <span className="tabular shrink-0 text-xs text-mesh-muted" title={exactTime(conversation.last_at)}>
              {relativeTime(conversation.last_at, new Date(now))}
            </span>
          </button>
        </li>
      ))}
    </ul>
  );
}

/** One conversation, oldest message at the top. */
function Thread({
  conversation,
  now,
  onBack,
}: {
  readonly conversation: Conversation;
  readonly now: number;
  readonly onBack: () => void;
}) {
  const query =
    conversation.partner === 'channel'
      ? `channel=${conversation.id}`
      : `with=${conversation.id}`;
  const thread = useResource<ConversationMessage[]>(`/messages/conversation?${query}&limit=200`);

  return (
    <div>
      <div className="flex items-baseline gap-3 border-b border-mesh-border px-4 py-2.5">
        <button
          type="button"
          onClick={onBack}
          className="text-sm text-mesh-accent hover:underline focus-visible:outline focus-visible:outline-2 focus-visible:outline-mesh-accent"
        >
          ← Alle Gespräche
        </button>
        {conversation.public_key === null ? (
          <span className="truncate text-sm text-mesh-text">
            {conversationTitle(conversation)}
          </span>
        ) : (
          <Link
            to={`/knoten/${conversation.public_key}`}
            className="truncate text-sm text-mesh-text hover:text-mesh-accent hover:underline focus-visible:outline focus-visible:outline-2 focus-visible:outline-mesh-accent"
            title="Alles über diesen Knoten"
          >
            {conversationTitle(conversation)}
          </Link>
        )}
        <span className="tabular ml-auto shrink-0 text-xs text-mesh-faint">
          {conversation.messages} {conversation.messages === 1 ? 'Nachricht' : 'Nachrichten'}
        </span>
      </div>

      {thread.data === null ? (
        <Loading what="Der Verlauf" />
      ) : thread.data.length === 0 ? (
        <Empty>Dieser Faden ist leer.</Empty>
      ) : (
        <ul className="space-y-2 p-4">
          {thread.data.map((message, index) => (
            <li
              key={`${message.at}-${index}`}
              className={message.direction === 'sent' ? 'flex justify-end' : 'flex'}
            >
              <div
                className={`max-w-[85%] rounded-lg border px-3 py-2 ${
                  message.direction === 'sent'
                    ? 'border-mesh-accent-dim bg-mesh-raised'
                    : 'border-mesh-border bg-mesh-surface'
                }`}
              >
                {/* Foreign text, rendered as text and never as markup. */}
                <p className="text-sm text-mesh-text">{message.text}</p>
                <p className="mt-1 flex flex-wrap items-center gap-x-2 gap-y-1 text-xs text-mesh-faint">
                  <span title={exactTime(message.at)}>
                    {relativeTime(message.at, new Date(now))}
                  </span>
                  {message.direction === 'received' ? (
                    <>
                      <SignalBars snr={message.snr} />
                      <span>
                        {message.stations === null
                          ? 'direkt'
                          : `über ${message.stations} ${message.stations === 1 ? 'Station' : 'Stationen'}`}
                      </span>
                    </>
                  ) : (
                    <span>
                      {message.flooded === null
                        ? 'gesendet'
                        : message.flooded
                          ? 'als Flood ausgesendet'
                          : 'über den bekannten Weg'}
                    </span>
                  )}
                </p>
              </div>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

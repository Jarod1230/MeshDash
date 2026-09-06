import type { ReactNode } from 'react';
import { Link } from 'react-router-dom';
import { SignalBars } from '../../ui/Signal';
import { Empty, Loading } from '../../ui/States';
import { exactTime, relativeTime } from '../../lib/time';
import { useResource } from '../../lib/useResource';
import { ThreadCompose, type ComposeTarget } from './ThreadCompose';
import { conversationTitle, type Conversation, type ConversationMessage } from './types';

/**
 * Who has been talked to in one area, as a list of threads.
 *
 * Received and sent are stored apart and were once shown apart, which cannot
 * show the one thing a conversation is: that an answer followed a question.
 * Here they are interleaved by time — still split between Direkt and Kanäle,
 * because a channel message has no sender the interface could name.
 */
export function ThreadList({
  threads,
  now,
  onSelect,
  empty,
  noMatch,
}: {
  readonly threads: readonly Conversation[];
  readonly now: number;
  readonly onSelect: (conversation: Conversation) => void;
  readonly empty: ReactNode;
  readonly noMatch: ReactNode | null;
}) {
  if (threads.length === 0) {
    return <Empty>{noMatch ?? empty}</Empty>;
  }

  return (
    <ul className="divide-y divide-mesh-border">
      {threads.map((conversation) => (
        <li key={`${conversation.partner}-${conversation.id}`}>
          <button
            type="button"
            onClick={() => onSelect(conversation)}
            className="flex w-full items-baseline gap-3 px-4 py-3 text-left hover:bg-mesh-raised focus-visible:outline focus-visible:outline-2 focus-visible:outline-mesh-accent"
          >
            <span className="min-w-0 flex-1">
              <span className="truncate text-mesh-text">{conversationTitle(conversation)}</span>
              <span className="mt-0.5 flex items-baseline gap-1.5 text-sm text-mesh-muted">
                {conversation.messages === 0 ? (
                  <span className="text-mesh-faint">Noch keine Nachricht — hier schreiben</span>
                ) : (
                  <>
                    {conversation.last_direction === 'sent' && (
                      <span className="text-xs text-mesh-faint">Sie:</span>
                    )}
                    <span className="truncate">{conversation.last_text}</span>
                  </>
                )}
              </span>
            </span>
            {conversation.messages > 0 && (
              <span
                className="tabular shrink-0 text-xs text-mesh-muted"
                title={exactTime(conversation.last_at)}
              >
                {relativeTime(conversation.last_at, new Date(now))}
              </span>
            )}
          </button>
        </li>
      ))}
    </ul>
  );
}

/** One conversation, oldest message at the top, compose fixed to this peer. */
export function ThreadView({
  conversation,
  now,
  onBack,
  onSent,
  backLabel,
}: {
  readonly conversation: Conversation;
  readonly now: number;
  readonly onBack: () => void;
  readonly onSent: () => void;
  readonly backLabel: string;
}) {
  const query =
    conversation.partner === 'channel'
      ? `channel=${conversation.id}`
      : `with=${conversation.id}`;
  const thread = useResource<ConversationMessage[]>(`/messages/conversation?${query}&limit=200`);

  const target: ComposeTarget =
    conversation.partner === 'channel'
      ? { kind: 'channel', index: Number(conversation.id) }
      : { kind: 'contact', prefix: conversation.id };

  return (
    <div>
      <div className="flex items-baseline gap-3 border-b border-mesh-border px-4 py-2.5">
        <button
          type="button"
          onClick={onBack}
          className="text-sm text-mesh-accent hover:underline focus-visible:outline focus-visible:outline-2 focus-visible:outline-mesh-accent"
        >
          ← {backLabel}
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
        <Empty>
          Dieser Faden ist noch leer. Schreiben Sie die erste Nachricht unten — Empfangenes und
          Gesendetes stehen danach im selben Verlauf.
        </Empty>
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

      <ThreadCompose target={target} onSent={() => onSent()} />
    </div>
  );
}

/** Compose-only view for a direct message to someone not yet in the list. */
export function NewDirectThread({
  onBack,
  onSent,
}: {
  readonly onBack: () => void;
  readonly onSent: (opened: { partner: 'contact' | 'channel'; id: string }) => void;
}) {
  return (
    <div>
      <div className="flex items-baseline gap-3 border-b border-mesh-border px-4 py-2.5">
        <button
          type="button"
          onClick={onBack}
          className="text-sm text-mesh-accent hover:underline focus-visible:outline focus-visible:outline-2 focus-visible:outline-mesh-accent"
        >
          ← Alle Direktnachrichten
        </button>
        <span className="truncate text-sm text-mesh-text">Neue Direktnachricht</span>
      </div>
      <Empty>
        Noch kein Faden — geben Sie das Schlüsselpräfix des Empfängers ein und schreiben Sie die
        erste Nachricht. Sobald sie hinaus ist, steht der Faden in der Liste.
      </Empty>
      <ThreadCompose target={{ kind: 'contact-new' }} onSent={onSent} />
    </div>
  );
}

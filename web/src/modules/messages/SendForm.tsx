import { useState, type FormEvent } from 'react';
import { apiPost, describeError, type ApiError } from '../../lib/api';
import type { Channel, SendResult } from './types';

/**
 * Sending, which is the first thing this interface does *to* the mesh.
 *
 * Two things are said plainly rather than hidden, because both surprise
 * people who expect a chat app:
 *
 * A direct message is answered with a receipt saying whether it went out as a
 * flood and how long the node thinks a reply will take. That is not delivery —
 * it means the node took it.
 *
 * A channel message gets no receipt at all. Nobody acknowledges a broadcast,
 * so there is nothing to wait for, and pretending otherwise would leave a
 * spinner running forever.
 */
export function SendForm({
  channels,
  onSent,
}: {
  readonly channels: readonly Channel[];
  readonly onSent: () => void;
}) {
  const [target, setTarget] = useState<'kontakt' | 'kanal'>('kanal');
  const [recipient, setRecipient] = useState('');
  const [channelIndex, setChannelIndex] = useState(channels[0]?.channel_index ?? 0);
  const [text, setText] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [receipt, setReceipt] = useState<SendResult | 'ohne Quittung' | null>(null);

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    setBusy(true);
    setError(null);
    setReceipt(null);

    try {
      if (target === 'kanal') {
        await apiPost('/messages/channel-send', { channel_index: channelIndex, text });
        setReceipt('ohne Quittung');
      } else {
        const result = await apiPost<SendResult>('/messages/send', {
          recipient_prefix: recipient.trim().toLowerCase(),
          text,
        });
        setReceipt(result);
      }
      setText('');
      onSent();
    } catch (cause) {
      setError(describeError(cause as ApiError));
    } finally {
      setBusy(false);
    }
  };

  return (
    <form onSubmit={submit} className="space-y-3 p-4">
      <div className="flex flex-wrap items-center gap-2">
        {(['kanal', 'kontakt'] as const).map((option) => (
          <button
            key={option}
            type="button"
            onClick={() => setTarget(option)}
            className={`rounded-md border px-3 py-1 text-sm capitalize focus-visible:outline focus-visible:outline-2 focus-visible:outline-mesh-accent ${
              target === option
                ? 'border-mesh-accent text-mesh-text'
                : 'border-mesh-border text-mesh-muted hover:text-mesh-text'
            }`}
          >
            {option}
          </button>
        ))}

        {target === 'kanal' ? (
          <select
            aria-label="Kanal"
            value={channelIndex}
            onChange={(event) => setChannelIndex(Number(event.target.value))}
            className="rounded-md border border-mesh-border bg-mesh-bg px-2 py-1 text-sm text-mesh-text"
          >
            {channels.length === 0 ? (
              <option value={0}>Kanal 0</option>
            ) : (
              channels.map((channel) => (
                <option key={channel.channel_index} value={channel.channel_index}>
                  {channel.name || `Kanal ${channel.channel_index}`}
                </option>
              ))
            )}
          </select>
        ) : (
          <input
            aria-label="Schlüsselpräfix des Empfängers"
            value={recipient}
            onChange={(event) => setRecipient(event.target.value)}
            placeholder="Schlüsselpräfix, 12 Hex-Zeichen"
            className="tabular w-56 rounded-md border border-mesh-border bg-mesh-bg px-2 py-1 text-sm text-mesh-text placeholder:text-mesh-faint"
          />
        )}
      </div>

      <div className="flex gap-2">
        <input
          aria-label="Nachricht"
          value={text}
          onChange={(event) => setText(event.target.value)}
          placeholder="Nachricht"
          className="flex-1 rounded-md border border-mesh-border bg-mesh-bg px-3 py-2 text-sm text-mesh-text placeholder:text-mesh-faint focus-visible:outline focus-visible:outline-2 focus-visible:outline-mesh-accent"
        />
        <button
          type="submit"
          disabled={busy || text.trim() === ''}
          className="rounded-md bg-mesh-accent px-4 py-2 text-sm text-mesh-bg disabled:opacity-50"
        >
          {busy ? 'Sendet …' : 'Senden'}
        </button>
      </div>

      {error !== null && (
        <p className="text-sm text-mesh-bad" role="alert">
          {error}
        </p>
      )}

      {receipt !== null && (
        <p className="text-sm text-mesh-muted" role="status">
          {receipt === 'ohne Quittung' ? (
            <>
              In den Kanal gegeben. Eine Rundsendung wird von niemandem bestätigt — ob sie jemand
              gehört hat, sagt der Node nicht.
            </>
          ) : receipt === null ? null : (
            <>
              Der Node hat die Nachricht übernommen
              {receipt.flooded ? ', als Flood ausgesendet' : ', über den bekannten Weg'}
              {receipt.expected_ack !== null && (
                <>
                  , Quittung <span className="tabular">{receipt.expected_ack}</span> erwartet in bis
                  zu {Math.round(receipt.estimated_timeout_ms / 1000)} Sekunden
                </>
              )}
              .
            </>
          )}
        </p>
      )}
    </form>
  );
}

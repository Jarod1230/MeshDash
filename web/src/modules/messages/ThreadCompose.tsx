import { useState, type FormEvent } from 'react';
import { apiPost, describeError, type ApiError } from '../../lib/api';
import type { SendResult } from './types';

/**
 * Where a reply goes — fixed by the open thread, never chosen from a global form.
 *
 * A brand-new direct message is the one exception: there is no peer yet, so the
 * compose field has to ask for a key prefix. Once it has been sent, the thread
 * list owns that peer like any other.
 */
export type ComposeTarget =
  | { kind: 'contact'; prefix: string }
  | { kind: 'contact-new' }
  | { kind: 'channel'; index: number };

/**
 * Sending from inside an open thread.
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
export function ThreadCompose({
  target,
  onSent,
}: {
  readonly target: ComposeTarget;
  readonly onSent: (opened: { partner: 'contact' | 'channel'; id: string }) => void;
}) {
  const [recipient, setRecipient] = useState('');
  const [text, setText] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [receipt, setReceipt] = useState<SendResult | 'ohne Quittung' | null>(null);

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    setError(null);
    setReceipt(null);

    setBusy(true);
    try {
      if (target.kind === 'channel') {
        await apiPost('/messages/channel-send', {
          channel_index: target.index,
          text,
        });
        setReceipt('ohne Quittung');
        setText('');
        onSent({ partner: 'channel', id: String(target.index) });
        return;
      }

      const prefix =
        target.kind === 'contact' ? target.prefix : recipient.trim().toLowerCase();
      if (prefix === '') {
        setError('Ohne Schlüsselpräfix weiß der Node nicht, wohin die Nachricht soll.');
        return;
      }

      const result = await apiPost<SendResult>('/messages/send', {
        recipient_prefix: prefix,
        text,
      });
      setReceipt(result);
      setText('');
      onSent({ partner: 'contact', id: prefix });
    } catch (cause) {
      setError(describeError(cause as ApiError));
    } finally {
      setBusy(false);
    }
  };

  const needsPrefix = target.kind === 'contact-new';
  const canSend = text.trim() !== '' && (!needsPrefix || recipient.trim() !== '');

  return (
    <form onSubmit={submit} className="space-y-2 border-t border-mesh-border p-4">
      {needsPrefix && (
        <input
          aria-label="Schlüsselpräfix des Empfängers"
          value={recipient}
          onChange={(event) => setRecipient(event.target.value)}
          placeholder="Schlüsselpräfix, 12 Hex-Zeichen"
          className="tabular w-full rounded-md border border-mesh-border bg-mesh-bg px-2 py-1.5 text-sm text-mesh-text placeholder:text-mesh-faint focus-visible:outline focus-visible:outline-2 focus-visible:outline-mesh-accent"
        />
      )}

      <div className="flex gap-2">
        <input
          aria-label="Nachricht"
          value={text}
          onChange={(event) => setText(event.target.value)}
          placeholder="Nachricht schreiben …"
          className="flex-1 rounded-md border border-mesh-border bg-mesh-bg px-3 py-2 text-sm text-mesh-text placeholder:text-mesh-faint focus-visible:outline focus-visible:outline-2 focus-visible:outline-mesh-accent"
        />
        <button
          type="submit"
          disabled={busy || !canSend}
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
          ) : (
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

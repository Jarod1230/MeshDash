import { useState } from 'react';
import { apiDelete, apiPut, describeError, type ApiError } from '../../lib/api';
import type { KnownContact } from './types';

/**
 * Where a node stands, when the node cannot say it itself.
 *
 * Most nodes carry no GPS, and of those that do, some report a position that
 * is plainly wrong. The operator usually knows — the repeater is on a named
 * hill. Without a way to write that down, the map stays a field with two dots
 * in it, and the operator has no way to fix that.
 *
 * A set position wins over the reported one and survives every advert. What
 * the node claims stays visible beside it: a node whose GPS puts it in the
 * wrong valley is worth noticing.
 */
export function PositionForm({
  contact,
  onSaved,
}: {
  readonly contact: KnownContact;
  readonly onSaved: () => void;
}) {
  const manual = contact.position_source === 'manual';
  const [latitude, setLatitude] = useState(() =>
    manual && contact.latitude !== null ? String(contact.latitude) : '',
  );
  const [longitude, setLongitude] = useState(() =>
    manual && contact.longitude !== null ? String(contact.longitude) : '',
  );
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const save = async (event: React.FormEvent) => {
    event.preventDefault();
    const lat = Number(latitude.replace(',', '.'));
    const lon = Number(longitude.replace(',', '.'));

    // Checked here as well as in the backend, because the message can be more
    // specific about what was typed than a rejected request can.
    if (latitude.trim() === '' || longitude.trim() === '' || Number.isNaN(lat) || Number.isNaN(lon)) {
      setError('Breite und Länge als Dezimalzahlen, etwa 48.137 und 11.576.');
      return;
    }

    setBusy(true);
    try {
      await apiPut('/nodes/position', {
        public_key: contact.public_key,
        latitude: lat,
        longitude: lon,
      });
      setError(null);
      onSaved();
    } catch (cause) {
      setError(describeError(cause as ApiError));
    } finally {
      setBusy(false);
    }
  };

  const clear = async () => {
    setBusy(true);
    try {
      await apiDelete('/nodes/position', { public_key: contact.public_key });
      setLatitude('');
      setLongitude('');
      setError(null);
      onSaved();
    } catch (cause) {
      setError(describeError(cause as ApiError));
    } finally {
      setBusy(false);
    }
  };

  return (
    <form onSubmit={save} className="flex flex-col gap-3">
      <div className="flex flex-wrap items-end gap-3">
        <label className="flex flex-col gap-1 text-xs text-mesh-faint">
          Breite
          <input
            value={latitude}
            onChange={(event) => setLatitude(event.target.value)}
            inputMode="decimal"
            placeholder="48.137"
            className="tabular w-32 rounded-md border border-mesh-border bg-mesh-bg px-2 py-1 text-sm text-mesh-text placeholder:text-mesh-faint focus-visible:outline focus-visible:outline-2 focus-visible:outline-mesh-accent"
          />
        </label>
        <label className="flex flex-col gap-1 text-xs text-mesh-faint">
          Länge
          <input
            value={longitude}
            onChange={(event) => setLongitude(event.target.value)}
            inputMode="decimal"
            placeholder="11.576"
            className="tabular w-32 rounded-md border border-mesh-border bg-mesh-bg px-2 py-1 text-sm text-mesh-text placeholder:text-mesh-faint focus-visible:outline focus-visible:outline-2 focus-visible:outline-mesh-accent"
          />
        </label>
        <button
          type="submit"
          disabled={busy}
          className="rounded-md border border-mesh-accent px-3 py-1.5 text-sm text-mesh-text hover:bg-mesh-raised disabled:text-mesh-faint focus-visible:outline focus-visible:outline-2 focus-visible:outline-mesh-accent"
        >
          Position setzen
        </button>
        {manual && (
          <button
            type="button"
            onClick={clear}
            disabled={busy}
            className="rounded-md border border-mesh-border px-3 py-1.5 text-sm text-mesh-muted hover:text-mesh-text disabled:text-mesh-faint focus-visible:outline focus-visible:outline-2 focus-visible:outline-mesh-accent"
          >
            Zurücknehmen
          </button>
        )}
      </div>

      {error !== null && (
        <p className="text-xs text-mesh-bad" role="alert">
          {error}
        </p>
      )}

      <p className="text-xs text-mesh-faint">
        {manual
          ? 'Diese Position hast du gesetzt. Sie bleibt, auch wenn der Knoten etwas anderes meldet.'
          : contact.position_source === 'reported'
            ? 'Diese Position meldet der Knoten selbst. Ein eigener Wert überschreibt sie hier — und nur hier.'
            : 'Dieser Knoten meldet keine Position. Ohne eine steht er auf keiner Karte.'}
        {contact.position_source === 'manual' && contact.reported_latitude !== null && (
          <>
            {' '}
            Der Knoten selbst meldet{' '}
            <span className="tabular">
              {contact.reported_latitude.toFixed(5)}, {contact.reported_longitude?.toFixed(5)}
            </span>
            .
          </>
        )}
      </p>
    </form>
  );
}

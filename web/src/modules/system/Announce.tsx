import { useState } from 'react';
import { apiPost, apiPut, describeError, type ApiError } from '../../lib/api';

/**
 * The two things an operator does to their own node, side by side.
 *
 * Placing it and announcing it belong together: a position the mesh never
 * hears about changes nothing out there, and an advert without a position
 * puts a node on nobody's map. Neither happens on a timer — both transmit,
 * and the band is shared.
 *
 * This is the one position in MeshDash a human types. It is a setting on the
 * node, not a pin on a map: what the map draws still comes back out of the
 * mesh, in this node's own advert like everyone else's. See ADR-0013.
 */
export function Announce({
  latitude,
  longitude,
  onChanged,
}: {
  readonly latitude: number | null;
  readonly longitude: number | null;
  readonly onChanged: () => void;
}) {
  const [north, setNorth] = useState(latitude === null ? '' : latitude.toFixed(6));
  const [east, setEast] = useState(longitude === null ? '' : longitude.toFixed(6));
  const [saving, setSaving] = useState(false);
  const [advertising, setAdvertising] = useState(false);
  const [note, setNote] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const save = async () => {
    const parsedNorth = Number(north.replace(',', '.'));
    const parsedEast = Number(east.replace(',', '.'));

    // Checked here so a typo comes back as a sentence instead of as a
    // rejection from the node — same bounds the firmware enforces.
    if (north.trim() === '' || east.trim() === '' || !isFinite(parsedNorth) || !isFinite(parsedEast)) {
      setError('Beide Felder brauchen eine Zahl in Grad, etwa 54.331026.');
      return;
    }
    if (Math.abs(parsedNorth) > 90 || Math.abs(parsedEast) > 180) {
      setError('Breite liegt zwischen -90 und 90, Länge zwischen -180 und 180.');
      return;
    }

    setSaving(true);
    try {
      await apiPut('/system/position', { latitude: parsedNorth, longitude: parsedEast });
      setError(null);
      setNote('Der Node hat die Position übernommen. Ins Mesh kommt sie mit dem nächsten Advert.');
      onChanged();
    } catch (cause) {
      setError(describeError(cause as ApiError));
    } finally {
      setSaving(false);
    }
  };

  const announce = async (flood: boolean) => {
    setAdvertising(true);
    try {
      await apiPost('/nodes/advert', { flood });
      setError(null);
      setNote(
        flood
          ? 'Advert geflutet. Wer es hört, kennt diesen Node jetzt — auch jenseits der Hörweite.'
          : 'Advert an die Nachbarschaft gesendet. Es reicht so weit, wie dieser Node zu hören ist.',
      );
    } catch (cause) {
      setError(describeError(cause as ApiError));
    } finally {
      setAdvertising(false);
    }
  };

  return (
    <div className="mt-5 border-t border-mesh-border pt-4">
      <div className="flex flex-wrap items-end gap-x-4 gap-y-3">
        <Field label="Breite" value={north} onChange={setNorth} placeholder="54.331026" />
        <Field label="Länge" value={east} onChange={setEast} placeholder="13.070254" />
        <Action onClick={save} busy={saving} busyLabel="wird gesetzt …">
          Position setzen
        </Action>
      </div>

      <p className="mt-2 text-xs text-mesh-faint">
        Die einzige Position, die hier jemand einträgt — und zwar am Node, nicht auf der Karte.
        Gezeichnet wird sie erst, wenn sie als Advert zurückkommt, wie bei jedem anderen Knoten
        auch. Das Setzen allein sendet nichts.
      </p>

      <div className="mt-4 flex flex-wrap items-center gap-3">
        <Action onClick={() => announce(false)} busy={advertising} busyLabel="wird gesendet …">
          An die Nachbarschaft
        </Action>
        <Action onClick={() => announce(true)} busy={advertising} busyLabel="wird gesendet …">
          Durch das ganze Mesh
        </Action>
        <span className="text-xs text-mesh-faint">
          Ein Advert stellt diesen Node vor. Geflutet reicht es weiter und kostet mehr Sendezeit —
          jeder Repeater in Reichweite gibt es weiter.
        </span>
      </div>

      {error !== null && (
        <p className="mt-3 text-xs text-mesh-bad" role="alert">
          {error}
        </p>
      )}
      {error === null && note !== null && (
        <p className="mt-3 text-xs text-mesh-accent" role="status">
          {note}
        </p>
      )}
    </div>
  );
}

function Field({
  label,
  value,
  onChange,
  placeholder,
}: {
  readonly label: string;
  readonly value: string;
  readonly onChange: (value: string) => void;
  readonly placeholder: string;
}) {
  return (
    <label className="block">
      <span className="block text-xs uppercase tracking-wider text-mesh-faint">{label}</span>
      <input
        type="text"
        inputMode="decimal"
        value={value}
        placeholder={placeholder}
        onChange={(event) => onChange(event.target.value)}
        className="tabular mt-0.5 w-32 rounded-md border border-mesh-border bg-mesh-bg px-2 py-1.5 text-sm text-mesh-text placeholder:text-mesh-faint focus-visible:outline focus-visible:outline-2 focus-visible:outline-mesh-accent"
      />
    </label>
  );
}

function Action({
  onClick,
  busy,
  busyLabel,
  children,
}: {
  readonly onClick: () => void;
  readonly busy: boolean;
  readonly busyLabel: string;
  readonly children: string;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={busy}
      className="rounded-md border border-mesh-accent px-3 py-1.5 text-sm text-mesh-text hover:bg-mesh-raised disabled:border-mesh-border disabled:text-mesh-faint focus-visible:outline focus-visible:outline-2 focus-visible:outline-mesh-accent"
    >
      {busy ? busyLabel : children}
    </button>
  );
}

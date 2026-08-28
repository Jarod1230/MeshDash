import { useState } from 'react';
import { apiPut, describeError, type ApiError } from '../../lib/api';
import { Failed, Loading } from '../../ui/States';
import { useResource } from '../../lib/useResource';

/**
 * What an operator can change without touching a file.
 *
 * # Why only some of it
 *
 * Where MeshDash listens, which device the node hangs on, where the database
 * lives — those decide how the process starts. A page the process serves
 * cannot change them, and offering them would be a promise it cannot keep.
 * They stay in `meshdash.toml`, and this page says so rather than leaving a
 * gap somebody has to guess about.
 *
 * # The options are named here, not discovered
 *
 * A section is a module's own business, and an option needs a sentence
 * explaining what it costs — a request over the air, a growing file. That
 * sentence has to be written by somebody, so the list lives here beside it.
 */
interface ModuleView {
  readonly module: string;
  readonly values: Record<string, unknown>;
  readonly changed: boolean;
}

interface AllSettings {
  readonly modules: readonly ModuleView[];
}

/** One option, as this page explains and edits it. */
interface Option {
  readonly key: string;
  readonly label: string;
  readonly help: string;
  readonly kind: 'switch' | 'number';
  /** For numbers: what the field will not go outside. */
  readonly range?: readonly [number, number];
  readonly unit?: string;
}

const SECTIONS: readonly {
  readonly module: string;
  readonly title: string;
  readonly summary: string;
  readonly options: readonly Option[];
}[] = [
  {
    module: 'telemetry',
    title: 'Nachbarn nach ihren Messwerten fragen',
    summary:
      'MeshCore sendet fremde Telemetrie nicht von selbst — sie muss angefordert werden. Jede Anfrage geht über Funk und belegt Sendezeit im Band, das sich das ganze Mesh teilt.',
    options: [
      {
        key: 'neighbours',
        label: 'Nachbarn fragen',
        help: 'Aus, solange niemand es einschaltet. Zu senden ist eine Entscheidung des Betreibers, keine Voreinstellung.',
        kind: 'switch',
      },
      {
        key: 'every_minutes',
        label: 'Abstand zwischen zwei Anfragen',
        help: 'Es wird immer nur ein Knoten pro Runde gefragt, reihum. Bei zehn erreichbaren Nachbarn und 30 Minuten dauert eine volle Runde fünf Stunden — für eine Batteriekurve reicht das und schont das Band.',
        kind: 'number',
        range: [1, 1440],
        unit: 'Minuten',
      },
      {
        key: 'silent_after_hours',
        label: 'Stille übergehen nach',
        help: 'Wer sich so lange nicht gemeldet hat, wird übersprungen. An etwas zu senden, das nicht da ist, kostet nur Sendezeit.',
        kind: 'number',
        range: [1, 8760],
        unit: 'Stunden',
      },
    ],
  },
  {
    module: 'traffic',
    title: 'Gehörte Pakete aufbewahren',
    summary:
      'Der Node meldet jedes Paket, das er empfängt — auch fremdes und verworfenes. Ob MeshDash den Verlauf behält, ist diese Entscheidung; wer wen direkt hört, wird ohnehin verdichtet festgehalten und unterliegt keiner Frist.',
    options: [
      {
        key: 'record',
        label: 'Verlauf mitschreiben',
        help: 'Aus heißt: Die Verdichtung entsteht weiter, der einzelne Paketverlauf nicht. Ohne ihn gibt es später nichts nachzuvollziehen.',
        kind: 'switch',
      },
      {
        key: 'keep_days',
        label: 'Verlauf aufbewahren',
        help: 'Älteres wird stündlich entfernt. Großzügig gewählt, weil MeshDash ein Analysewerkzeug ist — wer eine Störung von vorletzter Woche nachvollziehen will, braucht die Pakete und nicht ihre Zusammenfassung.',
        kind: 'number',
        range: [1, 3650],
        unit: 'Tage',
      },
    ],
  },
];

export function SettingsPage() {
  const settings = useResource<AllSettings>('/settings');
  const [saving, setSaving] = useState<string | null>(null);
  const [failure, setFailure] = useState<string | null>(null);

  const change = async (module: string, key: string, value: unknown) => {
    setSaving(`${module}.${key}`);
    try {
      await apiPut(`/settings/${module}`, { [key]: value });
      setFailure(null);
      settings.reload();
    } catch (cause) {
      setFailure(describeError(cause as ApiError));
    } finally {
      setSaving(null);
    }
  };

  if (settings.error !== null && settings.data === null) {
    return (
      <div className="rounded-lg border border-mesh-border bg-mesh-surface">
        <Failed error={settings.error} onRetry={settings.reload} />
      </div>
    );
  }

  if (settings.data === null) {
    return (
      <div className="rounded-lg border border-mesh-border bg-mesh-surface">
        <Loading what="Die Einstellungen" />
      </div>
    );
  }

  return (
    <div className="space-y-4">
      {failure !== null && (
        <p className="rounded-lg border border-mesh-bad px-4 py-3 text-sm text-mesh-bad" role="alert">
          {failure}
        </p>
      )}

      {SECTIONS.map((section) => {
        const view = settings.data?.modules.find((one) => one.module === section.module);

        return (
          <section
            key={section.module}
            className="rounded-lg border border-mesh-border bg-mesh-surface"
          >
            <header className="border-b border-mesh-border px-4 py-3">
              <div className="flex flex-wrap items-baseline gap-x-3">
                <h2 className="text-sm text-mesh-text">{section.title}</h2>
                {view?.changed === true && (
                  <span
                    className="text-xs text-mesh-faint"
                    title="Hier geändert; in meshdash.toml steht noch etwas anderes"
                  >
                    hier geändert
                  </span>
                )}
              </div>
              <p className="mt-1 max-w-2xl text-xs text-mesh-muted">{section.summary}</p>
            </header>

            <ul className="divide-y divide-mesh-border">
              {section.options.map((option) => (
                <li key={option.key} className="px-4 py-3">
                  <Field
                    option={option}
                    value={view?.values[option.key]}
                    busy={saving === `${section.module}.${option.key}`}
                    onChange={(value) => void change(section.module, option.key, value)}
                  />
                </li>
              ))}
            </ul>
          </section>
        );
      })}

      <p className="max-w-2xl text-xs text-mesh-faint">
        Was hier fehlt, entscheidet, wie der Dienst startet: die Adresse, an der er lauscht, der
        serielle Port des Node, der Ort der Datenbank und die Kartenquelle. Eine Seite, die dieser
        Dienst ausliefert, kann das nicht ändern — es steht in{' '}
        <span className="tabular">meshdash.toml</span> und gilt ab dem nächsten Start.
      </p>
    </div>
  );
}

function Field({
  option,
  value,
  busy,
  onChange,
}: {
  readonly option: Option;
  readonly value: unknown;
  readonly busy: boolean;
  readonly onChange: (value: unknown) => void;
}) {
  const [draft, setDraft] = useState<string | null>(null);

  if (option.kind === 'switch') {
    const on = value === true;

    return (
      <div className="flex items-start justify-between gap-4">
        <div>
          <span className="text-sm text-mesh-text">{option.label}</span>
          <p className="mt-0.5 max-w-xl text-xs text-mesh-faint">{option.help}</p>
        </div>
        <button
          type="button"
          role="switch"
          aria-checked={on}
          aria-label={option.label}
          disabled={busy}
          onClick={() => onChange(!on)}
          className={`mt-0.5 shrink-0 rounded-md border px-3 py-1 text-sm focus-visible:outline focus-visible:outline-2 focus-visible:outline-mesh-accent ${
            on
              ? 'border-mesh-accent text-mesh-text'
              : 'border-mesh-border text-mesh-muted hover:text-mesh-text'
          }`}
        >
          {busy ? '…' : on ? 'ein' : 'aus'}
        </button>
      </div>
    );
  }

  const [low, high] = option.range ?? [1, Number.MAX_SAFE_INTEGER];
  const shown = draft ?? String(value ?? '');

  const commit = () => {
    const parsed = Number(shown);
    setDraft(null);
    // Out of range or not a number: leave what is stored alone rather than
    // sending something the module would refuse anyway.
    if (!Number.isFinite(parsed) || parsed < low || parsed > high) return;
    if (parsed === value) return;
    onChange(parsed);
  };

  return (
    <div className="flex items-start justify-between gap-4">
      <div>
        <label className="text-sm text-mesh-text" htmlFor={`option-${option.key}`}>
          {option.label}
        </label>
        <p className="mt-0.5 max-w-xl text-xs text-mesh-faint">{option.help}</p>
      </div>
      <span className="mt-0.5 flex shrink-0 items-center gap-2">
        <input
          id={`option-${option.key}`}
          type="number"
          min={low}
          max={high}
          value={shown}
          disabled={busy}
          onChange={(event) => setDraft(event.target.value)}
          onBlur={commit}
          onKeyDown={(event) => {
            if (event.key === 'Enter') event.currentTarget.blur();
          }}
          className="tabular w-24 rounded-md border border-mesh-border bg-mesh-bg px-2 py-1 text-sm text-mesh-text focus-visible:outline focus-visible:outline-2 focus-visible:outline-mesh-accent"
        />
        <span className="text-xs text-mesh-faint">{option.unit}</span>
      </span>
    </div>
  );
}

import { useCallback, useEffect, useState, type FormEvent, type ReactNode } from 'react';
import { apiGet } from '../lib/api';
import { readToken, writeToken } from '../lib/token';

/**
 * Asks for the token, but only when the service actually wants one.
 *
 * MeshDash runs unauthenticated on loopback by default (ADR-0006), which is
 * the common case at home. Showing a sign-in form to someone whose service
 * has no token would invent a lock for a door that stands open, so the gate
 * probes first and stays out of the way when the answer is yes.
 */
type Gate =
  | { state: 'checking' }
  | { state: 'open' }
  | { state: 'needs-token'; rejected: boolean };

export function TokenGate({ children }: { readonly children: ReactNode }) {
  const [gate, setGate] = useState<Gate>({ state: 'checking' });
  const [entry, setEntry] = useState('');
  const [busy, setBusy] = useState(false);

  // Any authenticated route would do; the system status is the cheapest.
  const probe = useCallback(async (rejectedBefore: boolean) => {
    try {
      await apiGet('/system/status');
      setGate({ state: 'open' });
    } catch (cause) {
      const error = cause as { kind?: string };
      if (error.kind === 'unauthorized') {
        setGate({ state: 'needs-token', rejected: rejectedBefore });
      } else {
        // The service being down is not an authentication problem. Let the
        // pages themselves report it, where the message belongs.
        setGate({ state: 'open' });
      }
    }
  }, []);

  // In an effect, not during render: probing while rendering would set state
  // on every pass and spin.
  useEffect(() => {
    // See the note in useResource: the state changes inside `probe` all
    // happen after an await, and the microtask makes that visible.
    queueMicrotask(() => void probe(false));
  }, [probe]);

  if (gate.state === 'checking') {
    return <p className="p-8 text-sm text-mesh-muted">Verbindung wird geprüft …</p>;
  }

  if (gate.state === 'open') return <>{children}</>;

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    setBusy(true);
    writeToken(entry.trim() === '' ? null : entry.trim());
    await probe(true);
    setBusy(false);
  };

  return (
    <div className="mx-auto flex min-h-screen max-w-md flex-col justify-center px-6">
      <h1 className="text-lg text-mesh-text">Token erforderlich</h1>
      <p className="mt-2 text-sm text-mesh-muted">
        Dieser Dienst ist geschützt. Das Token steht in der Konfiguration des Servers unter{' '}
        <code className="text-mesh-accent">[auth] token</code>.
      </p>

      <form onSubmit={submit} className="mt-6">
        <label htmlFor="token" className="text-xs uppercase tracking-wider text-mesh-muted">
          Token
        </label>
        <input
          id="token"
          type="password"
          value={entry}
          onChange={(event) => setEntry(event.target.value)}
          autoComplete="current-password"
          className="mt-1 w-full rounded-md border border-mesh-border bg-mesh-surface px-3 py-2 text-mesh-text focus-visible:outline focus-visible:outline-2 focus-visible:outline-mesh-accent"
        />
        {gate.rejected && readToken() !== null && (
          <p className="mt-2 text-sm text-mesh-bad" role="alert">
            Das Token wurde nicht akzeptiert.
          </p>
        )}
        <button
          type="submit"
          disabled={busy}
          className="mt-4 w-full rounded-md bg-mesh-accent px-3 py-2 text-mesh-bg disabled:opacity-60"
        >
          {busy ? 'Wird geprüft …' : 'Anmelden'}
        </button>
      </form>
    </div>
  );
}

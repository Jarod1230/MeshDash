import { modules } from '../modules';

/**
 * Application shell.
 *
 * Scaffolding only — it renders the navigation from the module registry and
 * says so. Routing, live updates and the real dashboard arrive in step 7 of
 * docs/roadmap.md.
 */
export function App() {
  return (
    <div className="flex min-h-screen bg-mesh-bg text-mesh-text">
      <nav className="w-56 shrink-0 border-r border-mesh-border p-5">
        <div className="mb-6 text-lg font-semibold tracking-tight">MeshDash</div>
        {modules.length === 0 ? (
          <p className="text-sm text-mesh-muted">Noch keine Module registriert.</p>
        ) : (
          <ul className="space-y-1">
            {modules.map((m) => (
              <li key={m.id} className="text-sm text-mesh-muted">
                {m.title}
              </li>
            ))}
          </ul>
        )}
      </nav>

      <main className="flex-1 p-8">
        <h1 className="text-2xl font-semibold tracking-tight">Gerüst</h1>
        <p className="mt-3 max-w-prose text-mesh-muted">
          Das Frontend-Gerüst steht und baut. Funktionalität gibt es noch keine — die
          Modul-Registry ist leer und wird ab Schritt 6 der Roadmap gefüllt.
        </p>
        <p className="mt-6 text-sm text-mesh-muted">
          Nächste Schritte: <code className="text-mesh-accent">docs/roadmap.md</code>
        </p>
      </main>
    </div>
  );
}

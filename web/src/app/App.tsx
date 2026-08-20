import { NavLink, Route, Routes, useLocation } from 'react-router-dom';
import { modules } from '../modules';
import { ThemeToggle } from './Theme';
import { TokenGate } from './TokenGate';

/**
 * Application shell.
 *
 * Navigation comes from the module registry and from nowhere else: adding a
 * page means adding a module, never touching this file. See
 * docs/module-system.md.
 */
export function App() {
  return (
    <TokenGate>
      <div className="min-h-screen bg-mesh-bg text-mesh-text">
        <Header />
        <main className="mx-auto max-w-6xl px-4 py-6 sm:px-6">
          <PageTitle />
          <Routes>
            {modules.map((module) => (
              <Route key={module.id} path={module.path} element={<module.component />} />
            ))}
            <Route path="*" element={<NotFound />} />
          </Routes>
        </main>
      </div>
    </TokenGate>
  );
}

function Header() {
  return (
    <header className="border-b border-mesh-border bg-mesh-surface">
      <div className="mx-auto flex max-w-6xl items-center gap-5 px-4 sm:px-6">
        <span className="py-3 text-sm uppercase tracking-[0.14em] text-mesh-accent">MeshDash</span>
        <nav aria-label="Hauptnavigation" className="-mb-px flex flex-wrap gap-1 self-end">
          {modules.map((module) => (
            <NavLink
              key={module.id}
              to={module.path}
              end={module.path === '/'}
              className={({ isActive }) =>
                // The active tab is marked by the accent and a rule under it
                // rather than by a filled box: on the light theme a filled box
                // sits at 0.96 against a 1.0 surface and all but disappears.
                `border-b-2 px-2.5 py-1.5 text-sm transition-colors focus-visible:outline focus-visible:outline-2 focus-visible:outline-mesh-accent ${
                  isActive
                    ? 'border-mesh-accent text-mesh-text'
                    : 'border-transparent text-mesh-muted hover:text-mesh-text'
                }`
              }
            >
              {module.title}
            </NavLink>
          ))}
        </nav>
        <span className="flex-1" />
        <ThemeToggle />
      </div>
    </header>
  );
}

/** The current module's summary, so every page says what it answers. */
function PageTitle() {
  const { pathname } = useLocation();
  const current = modules.find((module) => module.path === pathname);
  if (current === undefined) return null;

  return (
    <div className="mb-5">
      <h1 className="text-xl text-mesh-text">{current.title}</h1>
      <p className="mt-0.5 text-sm text-mesh-muted">{current.summary}</p>
    </div>
  );
}

function NotFound() {
  return (
    <div className="rounded-lg border border-mesh-border bg-mesh-surface px-4 py-6">
      <p className="text-mesh-text">Diese Seite gibt es nicht.</p>
      <p className="mt-1 text-sm text-mesh-muted">
        Vielleicht gehört sie zu einem Modul, das nicht geladen ist.
      </p>
    </div>
  );
}

import { useEffect } from 'react';
import { NavLink, Route, Routes, useLocation, useNavigate } from 'react-router-dom';
import { modules } from '../modules';
import { Ground } from '../ground/Ground';
import { EventStream, useStreamLive } from '../lib/events';
import { ThemeToggle } from './Theme';
import { TokenGate } from './TokenGate';

/**
 * Application shell.
 *
 * # The map is the ground, not a page
 *
 * MeshDash opens on the surface and stays on it. The pages sit above it as a
 * shutter that opens and closes; the drawing underneath is never unmounted, so
 * the section an operator was looking at is still there when they come back —
 * see ADR-0011.
 *
 * The address still carries the state, and it still carries it as a path: a
 * link to a node opens the same thing a click on it does. ADR-0011 sketched
 * query parameters for this; ADR-0014 says why the path stayed a path.
 *
 * Navigation comes from the module registry and from nowhere else: adding a
 * page means adding a module, never touching this file. See
 * docs/module-system.md.
 */
export function App() {
  return (
    <TokenGate>
      <EventStream>
        <div className="relative h-dvh w-full overflow-hidden bg-mesh-bg text-mesh-text">
          <Ground />
          {/* The shutter first, the controls above it: the tabs have to stay
              usable while a page is open, or switching pages would mean
              closing one first. */}
          <Shutter />
          <Overlay />
        </div>
      </EventStream>
    </TokenGate>
  );
}

/**
 * The controls, floating in the corners.
 *
 * In the corners because the middle belongs to the map. Nothing here has a
 * background wider than its content, so the surface stays readable behind it.
 */
function Overlay() {
  return (
    <div className="pointer-events-none absolute inset-x-0 top-0 z-20 flex flex-wrap items-start justify-between gap-3 p-3">
      <span className="pointer-events-auto rounded-md bg-mesh-surface/80 px-2.5 py-1.5 text-sm uppercase tracking-[0.14em] text-mesh-accent backdrop-blur">
        MeshDash
      </span>

      <div className="pointer-events-auto flex flex-wrap items-center gap-3 rounded-md bg-mesh-surface/80 px-2.5 py-1.5 backdrop-blur">
        <nav aria-label="Hauptnavigation" className="flex gap-1">
          {modules.map((module) => (
            <NavLink
              key={module.id}
              to={module.path}
              className={({ isActive }) =>
                `shrink-0 rounded px-2 py-1 text-sm transition-colors focus-visible:outline focus-visible:outline-2 focus-visible:outline-mesh-accent ${
                  isActive ? 'text-mesh-accent' : 'text-mesh-muted hover:text-mesh-text'
                }`
              }
            >
              {module.title}
            </NavLink>
          ))}
        </nav>
        <LiveIndicator />
        <ThemeToggle />
      </div>
    </div>
  );
}

/**
 * The pages, laid over the surface.
 *
 * Closed means the address is `/` and there is nothing here at all — not a
 * hidden panel, so the surface gets the whole window and the pages stop
 * fetching when nobody is reading them.
 */
function Shutter() {
  const { pathname } = useLocation();
  const navigate = useNavigate();
  const open = pathname !== '/';

  useEffect(() => {
    if (!open) return;

    const close = (event: KeyboardEvent) => {
      if (event.key === 'Escape') navigate('/');
    };
    window.addEventListener('keydown', close);

    return () => window.removeEventListener('keydown', close);
  }, [open, navigate]);

  if (!open) return null;

  const current = modules.find(
    (module) => module.path === pathname || pathname.startsWith(`${module.path}/`),
  );

  return (
    <div className="absolute inset-0 z-10 flex justify-center overflow-y-auto bg-mesh-bg/70 pt-16 backdrop-blur-sm">
      <div className="mb-6 h-fit w-full max-w-6xl rounded-lg border border-mesh-border bg-mesh-surface p-4 shadow-lg sm:p-6">
        <div className="mb-5 flex items-start justify-between gap-4">
          <div>
            <h1 className="text-xl text-mesh-text">{current?.title ?? 'Nicht gefunden'}</h1>
            <p className="mt-0.5 text-sm text-mesh-muted">{current?.summary ?? ''}</p>
          </div>
          <button
            type="button"
            onClick={() => navigate('/')}
            className="shrink-0 rounded-md border border-mesh-border px-2.5 py-1 text-sm text-mesh-muted hover:text-mesh-text focus-visible:outline focus-visible:outline-2 focus-visible:outline-mesh-accent"
          >
            zur Karte · Esc
          </button>
        </div>

        <Routes>
          {modules.map((module) => (
            <Route
              key={module.id}
              // A trailing wildcard so a module can have pages below its own
              // path — a detail view, say — without the shell knowing any of
              // them.
              path={`${module.path}/*`}
              element={<module.component />}
            />
          ))}
          <Route path="*" element={<NotFound />} />
        </Routes>
      </div>
    </div>
  );
}

/**
 * Whether the page is currently being told about changes.
 *
 * Worth showing: without it, a stale page and a quiet mesh look exactly the
 * same. The wording says what is true — the stream is connected — rather than
 * claiming the data is current.
 */
function LiveIndicator() {
  const live = useStreamLive();

  return (
    <span
      className="flex items-center gap-1.5 text-xs text-mesh-muted"
      title={
        live
          ? 'Änderungen treffen sofort ein'
          : 'Kein Ereignisstrom — die Seiten laden nur beim Öffnen'
      }
    >
      <span
        className={`h-1.5 w-1.5 rounded-full ${live ? 'bg-mesh-accent' : 'bg-mesh-faint'}`}
        aria-hidden="true"
      />
      {live ? 'live' : 'nicht live'}
    </span>
  );
}

function NotFound() {
  return (
    <div className="px-1 py-2">
      <p className="text-mesh-text">Diese Seite gibt es nicht.</p>
      <p className="mt-1 text-sm text-mesh-muted">
        Vielleicht gehört sie zu einem Modul, das nicht geladen ist.
      </p>
    </div>
  );
}

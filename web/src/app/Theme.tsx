import { useCallback, useEffect, useState } from 'react';

type Theme = 'dark' | 'light';
const KEY = 'meshdash.theme';

/** Reads the stored choice, falling back to what the system asks for. */
function initialTheme(): Theme {
  try {
    const stored = window.localStorage.getItem(KEY);
    if (stored === 'dark' || stored === 'light') return stored;
  } catch {
    // Storage may be unavailable; the system preference still works.
  }
  return window.matchMedia('(prefers-color-scheme: light)').matches ? 'light' : 'dark';
}

/**
 * Dark and light, remembered.
 *
 * Dark is the default rather than the system preference alone: a mesh
 * dashboard is often read at night, next to a radio, and a white screen in a
 * dark room is its own kind of failure.
 */
export function useTheme() {
  const [theme, setTheme] = useState<Theme>(initialTheme);

  useEffect(() => {
    document.documentElement.dataset['theme'] = theme;
    try {
      window.localStorage.setItem(KEY, theme);
    } catch {
      // Not remembering the choice is survivable.
    }
  }, [theme]);

  const toggle = useCallback(() => {
    setTheme((current) => (current === 'dark' ? 'light' : 'dark'));
  }, []);

  return { theme, toggle };
}

export function ThemeToggle() {
  const { theme, toggle } = useTheme();

  return (
    <button
      type="button"
      onClick={toggle}
      className="rounded-md border border-mesh-border px-2.5 py-1 text-xs text-mesh-muted hover:bg-mesh-raised hover:text-mesh-text focus-visible:outline focus-visible:outline-2 focus-visible:outline-mesh-accent"
      aria-label={theme === 'dark' ? 'Zur hellen Ansicht wechseln' : 'Zur dunklen Ansicht wechseln'}
    >
      {theme === 'dark' ? 'Hell' : 'Dunkel'}
    </button>
  );
}

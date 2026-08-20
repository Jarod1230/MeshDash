import { useCallback, useEffect, useState } from 'react';
import { apiGet, type ApiError } from './api';

/** What a page knows about one piece of data. */
export interface Resource<T> {
  /** The data, or null while it has never loaded. */
  readonly data: T | null;
  /** True only until the first answer arrives — reloads do not flip it. */
  readonly loading: boolean;
  /** What went wrong last, or null. */
  readonly error: ApiError | null;
  /** Fetches again, keeping whatever is already shown. */
  readonly reload: () => void;
}

/**
 * Loads one API path and keeps it current.
 *
 * No caching library, by decision — see ADR-0008. Freshness here is not a
 * matter of elapsed time: the event stream says when something changed, and
 * the page calls `reload`. A cache with its own idea of staleness would sit
 * beside that and have to be reconciled with it.
 *
 * A reload deliberately keeps the previous data on screen. Blanking a table
 * because a refresh is in flight makes the interface flicker on a busy mesh,
 * where refreshes are constant.
 */
export function useResource<T>(path: string): Resource<T> {
  const [data, setData] = useState<T | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<ApiError | null>(null);

  const load = useCallback(
    async (signal: { cancelled: boolean }) => {
      try {
        const next = await apiGet<T>(path);
        if (signal.cancelled) return;
        setData(next);
        setError(null);
      } catch (cause) {
        if (signal.cancelled) return;
        setError(cause as ApiError);
      } finally {
        if (!signal.cancelled) setLoading(false);
      }
    },
    [path],
  );

  const [attempt, setAttempt] = useState(0);

  useEffect(() => {
    // Guards against a slow answer for a path the page has already left.
    const signal = { cancelled: false };
    // Started in a microtask: every setState inside `load` happens after an
    // await, but the lint rule cannot see through the async boundary, and
    // deferring the start makes that explicit rather than silenced.
    queueMicrotask(() => {
      if (!signal.cancelled) void load(signal);
    });
    return () => {
      signal.cancelled = true;
    };
  }, [load, attempt]);

  const reload = useCallback(() => setAttempt((value) => value + 1), []);

  return { data, loading, error, reload };
}

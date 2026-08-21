import { useCallback, useEffect, useState } from 'react';
import { apiGet, type ApiError } from './api';

/** The one thing a paged listing needs from its items: a cursor. */
export interface Identified {
  readonly id: number;
}

/** What a page knows about a listing it can walk backwards through. */
export interface PagedResource<T> {
  /** Everything loaded so far, newest first. Null until the first answer. */
  readonly items: readonly T[] | null;
  /** True only until the first answer arrives. */
  readonly loading: boolean;
  /** True while an older page is on its way. */
  readonly loadingMore: boolean;
  /** What went wrong last, or null. */
  readonly error: ApiError | null;
  /** Whether asking for an older page could still yield something. */
  readonly hasMore: boolean;
  /** Loads the next older page and appends it. */
  readonly loadMore: () => void;
  /** Throws away every loaded page and starts again at the newest. */
  readonly reload: () => void;
}

/**
 * Loads a listing page by page, oldest direction, by cursor.
 *
 * The backend pages by `?before=<id>` rather than by offset. An offset would
 * shift under us: every advert that arrives while the user reads pushes the
 * older rows down by one, so page two would repeat what page one showed. The
 * id of the last row on screen does not move.
 *
 * A reload starts over at the newest page instead of refetching every loaded
 * page. Refetching them all would multiply requests on a mesh where the event
 * stream fires constantly, and the older pages are what the user already read.
 */
export function usePagedResource<T extends Identified>(
  path: string,
  pageSize: number,
): PagedResource<T> {
  const [items, setItems] = useState<readonly T[] | null>(null);
  const [loading, setLoading] = useState(true);
  const [loadingMore, setLoadingMore] = useState(false);
  const [error, setError] = useState<ApiError | null>(null);
  const [hasMore, setHasMore] = useState(false);
  const [attempt, setAttempt] = useState(0);

  useEffect(() => {
    const signal = { cancelled: false };
    // Started in a microtask for the same reason as in `useResource`: every
    // setState happens after an await, which the lint rule cannot see.
    queueMicrotask(() => {
      if (signal.cancelled) return;
      void (async () => {
        try {
          const page = await apiGet<T[]>(pagePath(path, pageSize, null));
          if (signal.cancelled) return;
          setItems(page);
          setHasMore(page.length === pageSize);
          setError(null);
        } catch (cause) {
          if (signal.cancelled) return;
          setError(cause as ApiError);
        } finally {
          if (!signal.cancelled) setLoading(false);
        }
      })();
    });
    return () => {
      signal.cancelled = true;
    };
  }, [path, pageSize, attempt]);

  const loadMore = useCallback(() => {
    const oldest = items?.at(-1);
    if (oldest === undefined || loadingMore) return;
    setLoadingMore(true);
    void (async () => {
      try {
        const page = await apiGet<T[]>(pagePath(path, pageSize, oldest.id));
        // Appending to the latest state rather than to `items`: a live reload
        // may have replaced the list while this request was in flight.
        setItems((latest) => [...(latest ?? []), ...page]);
        setHasMore(page.length === pageSize);
        setError(null);
      } catch (cause) {
        setError(cause as ApiError);
      } finally {
        setLoadingMore(false);
      }
    })();
  }, [path, pageSize, items, loadingMore]);

  const reload = useCallback(() => setAttempt((value) => value + 1), []);

  return { items, loading, loadingMore, error, hasMore, loadMore, reload };
}

/** Builds the request path, keeping whatever query the caller already wrote. */
function pagePath(path: string, limit: number, before: number | null): string {
  const separator = path.includes('?') ? '&' : '?';
  const cursor = before === null ? '' : `&before=${before}`;
  return `${path}${separator}limit=${limit}${cursor}`;
}

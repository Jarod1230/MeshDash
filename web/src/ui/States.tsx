import type { ReactNode } from 'react';
import { describeError, type ApiError } from '../lib/api';

/**
 * What a page shows when it has nothing to show.
 *
 * Three cases, and they must not look alike: still loading, nothing there
 * yet, or something broke. Collapsing them into one spinner is how a broken
 * service ends up looking like an empty mesh.
 */

export function Loading({ what }: { readonly what: string }) {
  return (
    <p className="px-4 py-6 text-sm text-mesh-muted" role="status">
      {what} wird geladen …
    </p>
  );
}

/** An empty result. Says what would appear here, so the screen is not a dead end. */
export function Empty({ children }: { readonly children: ReactNode }) {
  return <p className="px-4 py-6 text-sm text-mesh-muted">{children}</p>;
}

/** A failure, with the reason and a way to try again. */
export function Failed({ error, onRetry }: { readonly error: ApiError; readonly onRetry?: () => void }) {
  return (
    <div className="px-4 py-6" role="alert">
      <p className="text-sm text-mesh-bad">{describeError(error)}</p>
      {onRetry !== undefined && (
        <button
          type="button"
          onClick={onRetry}
          className="mt-3 rounded-md border border-mesh-border px-3 py-1.5 text-sm text-mesh-text hover:bg-mesh-raised focus-visible:outline focus-visible:outline-2 focus-visible:outline-mesh-accent"
        >
          Erneut versuchen
        </button>
      )}
    </div>
  );
}

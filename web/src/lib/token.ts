/**
 * Where the bearer token lives.
 *
 * `localStorage` by decision, see ADR-0008: MeshDash is a dashboard people
 * keep open, and a token that has to be retyped after every restart ends up
 * written on a note beside the screen — worse than storing it.
 *
 * The trade accepted there brings one duty with it: never put foreign text
 * into the DOM as markup. Node names, message texts and channel names arrive
 * over the air from other people's devices.
 */
const KEY = 'meshdash.token';

/** The stored token, or null when none was ever entered. */
export function readToken(): string | null {
  try {
    return window.localStorage.getItem(KEY);
  } catch {
    // Private browsing modes can refuse storage entirely. Running without a
    // remembered token is worse than crashing, but only slightly.
    return null;
  }
}

/** Remembers a token, or forgets it when given null. */
export function writeToken(token: string | null): void {
  try {
    if (token === null) {
      window.localStorage.removeItem(KEY);
    } else {
      window.localStorage.setItem(KEY, token);
    }
  } catch {
    // Nothing to do — the session keeps working, it just will not be
    // remembered.
  }
}

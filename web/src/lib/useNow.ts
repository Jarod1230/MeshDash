import { useEffect, useState } from 'react';

/**
 * The current time, as a value that changes on its own.
 *
 * Reading `Date.now()` while rendering is impure — React may render at any
 * moment or not at all — and it has a visible consequence: "vor 2 Min" would
 * be written once and then stay wrong until something else caused a render.
 * A clock that ticks fixes both.
 *
 * Once a minute is enough for the resolution this interface shows.
 */
export function useNow(intervalMs = 60_000): number {
  const [now, setNow] = useState(() => Date.now());

  useEffect(() => {
    const timer = window.setInterval(() => setNow(Date.now()), intervalMs);
    return () => window.clearInterval(timer);
  }, [intervalMs]);

  return now;
}

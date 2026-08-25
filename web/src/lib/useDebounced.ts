import { useEffect, useState } from 'react';

/**
 * A value that only settles after typing stops.
 *
 * Search text goes into the request path, and a request per keystroke would
 * put six of them on the wire for one word — on a Raspberry Pi serving a mesh
 * that is six SQL scans nobody waited for.
 */
export function useDebounced<T>(value: T, delayMs = 250): T {
  const [settled, setSettled] = useState(value);

  useEffect(() => {
    const timer = window.setTimeout(() => setSettled(value), delayMs);
    return () => window.clearTimeout(timer);
  }, [value, delayMs]);

  return settled;
}

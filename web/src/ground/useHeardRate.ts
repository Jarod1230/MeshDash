import { useRef, useState } from 'react';
import { useLiveReload, type AppEvent } from '../lib/events';
import { isReceivedPacket } from '../lib/pushes';

/** How far back the rate looks. */
const WINDOW_MS = 60_000;

/**
 * How many packets the node heard in the last minute.
 *
 * The first question anybody asks a mesh dashboard is whether anything is
 * happening at all, and a quiet mesh and a broken connection look identical
 * without this. It counts events rather than reading them: what a packet was
 * is the backend's business, how many arrived is not.
 *
 * Deliberately a count and not a graph. A rate over a minute is what the eye
 * can use while looking at a map; the history is in the packet log.
 */
export function useHeardRate(): number {
  const seen = useRef<number[]>([]);
  const [rate, setRate] = useState(0);

  useLiveReload(
    (event: AppEvent) => event.type === 'push' && isReceivedPacket(event.payload),
    () => {
      const now = Date.now();
      seen.current = [...seen.current.filter((at) => now - at < WINDOW_MS), now];
      setRate(seen.current.length);
    },
  );

  return rate;
}

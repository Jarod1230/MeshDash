import { useMemo, useState } from 'react';
import { useNow } from './useNow';

/** The stretches of time the interface offers. */
export const RANGES = [
  { key: '1h', label: '1 Std', hours: 1 },
  { key: '24h', label: '24 Std', hours: 24 },
  { key: '7d', label: '7 Tage', hours: 24 * 7 },
  { key: '30d', label: '30 Tage', hours: 24 * 30 },
  { key: 'alles', label: 'alles', hours: null },
] as const;

export type RangeKey = (typeof RANGES)[number]['key'];

/** A chosen stretch of time and the query it turns into. */
export interface ChosenRange {
  /** Which one is chosen. */
  readonly key: RangeKey;
  /** Choose another one. */
  readonly choose: (key: RangeKey) => void;
  /**
   * `&since=…`, or an empty string for "alles".
   *
   * The separator belongs to the parameter, not to the caller: appending
   * `&${query}` would leave a trailing `&` behind whenever the range is open,
   * and an empty parameter is not the same as no parameter.
   */
  readonly query: string;
}

/**
 * Keeps which stretch of time a page shows.
 *
 * The lower bound is rounded down to the full minute so that the request path
 * only changes once a minute. Computed from the current millisecond it would
 * change on every render, and every render would start a new request.
 */
export function useTimeRange(initial: RangeKey = '24h'): ChosenRange {
  const [key, choose] = useState<RangeKey>(initial);
  // The ticking clock, not `Date.now()`: reading the time while rendering is
  // impure, and the path must only change when the clock says so.
  const minute = Math.floor(useNow() / 60_000);

  const query = useMemo(() => {
    const hours = RANGES.find((range) => range.key === key)?.hours ?? null;
    if (hours === null) return '';
    const since = new Date((minute - hours * 60) * 60_000).toISOString();
    return `&since=${encodeURIComponent(since)}`;
  }, [key, minute]);

  return { key, choose, query };
}

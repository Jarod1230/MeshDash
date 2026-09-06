import type { HeardBy } from './links';

/**
 * Time on the link layer — Stufe C „Zeit“.
 *
 * Timeless `/traffic/links` stays the default. A range is requested only when
 * the picker (or playback) is active, matching ADR-0018. Playback v1 is fixed
 * preset windows, not a scrubber: the same duration, ending now or earlier.
 */

/** Durations the map offers once a time filter is on. No "alles" — off is timeless. */
export const LINK_RANGES = [
  { key: '1h', label: '1 Std', hours: 1 },
  { key: '24h', label: '24 Std', hours: 24 },
  { key: '7d', label: '7 Tage', hours: 24 * 7 },
  { key: '30d', label: '30 Tage', hours: 24 * 30 },
] as const;

export type LinkRangeKey = (typeof LINK_RANGES)[number]['key'];

/**
 * Where the chosen window ends.
 *
 * `jetzt` means until ≈ now (rolling). The others shift `until` into the past
 * so the link layer shows the same stretch of time as it looked then.
 */
export const PLAYBACK_PRESETS = [
  { key: 'jetzt', label: 'jetzt', hoursAgo: 0 },
  { key: '1d', label: 'vor 1 Tag', hoursAgo: 24 },
  { key: '7d', label: 'vor 1 Woche', hoursAgo: 24 * 7 },
  { key: '14d', label: 'vor 2 Wochen', hoursAgo: 24 * 14 },
] as const;

export type PlaybackKey = (typeof PLAYBACK_PRESETS)[number]['key'];

/** Query key for the duration. Absent → timeless links. */
export const ZEIT_PARAM = 'zeit';

/** Query key for playback offset. Only written when not `jetzt`. */
export const ABSPIELEN_PARAM = 'abspielen';

/** What the backend wraps around `HeardBy[]` when since/until are set. */
export interface TimedLinksBody {
  readonly clamped: boolean;
  readonly keep_days: number;
  readonly effective_since: string;
  readonly effective_until: string;
  readonly links: readonly HeardBy[];
}

/** Normalised view the map draws from. */
export type TrafficLinksView =
  | { readonly mode: 'timeless'; readonly links: readonly HeardBy[] }
  | {
      readonly mode: 'timed';
      readonly clamped: boolean;
      readonly keep_days: number;
      readonly effective_since: string;
      readonly effective_until: string;
      readonly links: readonly HeardBy[];
    };

export interface LinksTimeChoice {
  /** `null` means the timeless summary. */
  readonly range: LinkRangeKey | null;
  readonly playback: PlaybackKey;
}

/** Reads map time state from the address. Unknown values fall back safely. */
export function readLinksTime(params: URLSearchParams): LinksTimeChoice {
  const rawRange = params.get(ZEIT_PARAM);
  const range = LINK_RANGES.find((entry) => entry.key === rawRange)?.key ?? null;

  const rawPlay = params.get(ABSPIELEN_PARAM);
  const playback =
    range === null
      ? 'jetzt'
      : (PLAYBACK_PRESETS.find((entry) => entry.key === rawPlay)?.key ?? 'jetzt');

  return { range, playback };
}

/**
 * Writes map time state into a copy of the query string.
 *
 * Only the deviation is stored: timeless clears both keys; `jetzt` clears
 * `abspielen` so a live window does not pollute the address.
 */
export function writeLinksTime(
  params: URLSearchParams,
  choice: LinksTimeChoice,
): URLSearchParams {
  const next = new URLSearchParams(params);

  if (choice.range === null) {
    next.delete(ZEIT_PARAM);
    next.delete(ABSPIELEN_PARAM);
    return next;
  }

  next.set(ZEIT_PARAM, choice.range);
  if (choice.playback === 'jetzt') next.delete(ABSPIELEN_PARAM);
  else next.set(ABSPIELEN_PARAM, choice.playback);

  return next;
}

/**
 * Builds the API path for the current choice.
 *
 * Bounds are floored to the full minute so the path only changes once a
 * minute — same reason as `useTimeRange`. Never call with `Date.now()` from
 * render; pass a clock value (`useNow`).
 */
export function trafficLinksPath(choice: LinksTimeChoice, nowMs: number): string {
  if (choice.range === null) return '/traffic/links';

  const hours = LINK_RANGES.find((entry) => entry.key === choice.range)?.hours;
  const offset = PLAYBACK_PRESETS.find((entry) => entry.key === choice.playback)?.hoursAgo;
  if (hours === undefined || offset === undefined) return '/traffic/links';

  const minute = Math.floor(nowMs / 60_000);
  const untilMs = (minute - offset * 60) * 60_000;
  const sinceMs = untilMs - hours * 3_600_000;

  const since = new Date(sinceMs).toISOString();
  const until = new Date(untilMs).toISOString();

  return `/traffic/links?since=${encodeURIComponent(since)}&until=${encodeURIComponent(until)}`;
}

/**
 * Turns the raw JSON into a view the map can draw.
 *
 * Timeless answers are a bare array; timed answers are the wrap object. The
 * caller says which shape it asked for — the browser does not invent one.
 */
export function parseTrafficLinksResponse(
  raw: unknown,
  timed: boolean,
): TrafficLinksView {
  if (!timed) {
    if (!Array.isArray(raw)) {
      throw new Error('timeless /traffic/links must be an array');
    }
    return { mode: 'timeless', links: raw as HeardBy[] };
  }

  if (raw === null || typeof raw !== 'object' || Array.isArray(raw)) {
    throw new Error('timed /traffic/links must be a wrap object');
  }

  const body = raw as Record<string, unknown>;
  if (!Array.isArray(body.links)) {
    throw new Error('timed /traffic/links.links must be an array');
  }
  if (typeof body.clamped !== 'boolean') {
    throw new Error('timed /traffic/links.clamped must be a boolean');
  }
  if (typeof body.keep_days !== 'number') {
    throw new Error('timed /traffic/links.keep_days must be a number');
  }
  if (typeof body.effective_since !== 'string' || typeof body.effective_until !== 'string') {
    throw new Error('timed /traffic/links effective bounds must be strings');
  }

  return {
    mode: 'timed',
    clamped: body.clamped,
    keep_days: body.keep_days,
    effective_since: body.effective_since,
    effective_until: body.effective_until,
    links: body.links as HeardBy[],
  };
}

/**
 * One German sentence when the server had to clamp the window.
 *
 * Empty string means nothing to disclose. Callers show this next to the empty
 * / capped states so retention is never silent (ADR-0018).
 */
export function clampReason(view: TrafficLinksView): string | null {
  if (view.mode !== 'timed' || !view.clamped) return null;

  const days = view.keep_days;
  const dayWord = days === 1 ? 'Tag' : 'Tage';

  return (
    `Der angeforderte Zeitraum liegt außerhalb der Aufbewahrung ` +
    `(${days} ${dayWord}) und wurde gekürzt. ` +
    `Angezeigt: ${shortStamp(view.effective_since)} – ${shortStamp(view.effective_until)}.`
  );
}

/** Compact stamp for overlay text; falls back to the raw string. */
function shortStamp(iso: string): string {
  const value = new Date(iso);
  if (Number.isNaN(value.getTime())) return iso;
  return value.toLocaleString('de-DE', {
    day: '2-digit',
    month: '2-digit',
    year: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  });
}

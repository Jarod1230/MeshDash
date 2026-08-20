/**
 * Time as an operator reads it.
 *
 * "vor 2 Min" answers the question people actually ask of a mesh — is this
 * node still there — where a timestamp makes them do the subtraction.
 */
const MINUTE = 60;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;

/** Formats the distance from `now` back to `iso` in German. */
export function relativeTime(iso: string, now: Date = new Date()): string {
  const then = new Date(iso);
  if (Number.isNaN(then.getTime())) {
    return 'unbekannt';
  }

  const seconds = Math.round((now.getTime() - then.getTime()) / 1000);

  // A clock that is slightly ahead should not read "in -3 seconds".
  if (seconds < 0) return 'gerade eben';
  if (seconds < 45) return 'gerade eben';
  if (seconds < HOUR) return `vor ${Math.round(seconds / MINUTE)} Min`;
  if (seconds < DAY) {
    const hours = Math.floor(seconds / HOUR);
    const minutes = Math.round((seconds % HOUR) / MINUTE);
    return minutes === 0 ? `vor ${hours} Std` : `vor ${hours} Std ${minutes} Min`;
  }

  const days = Math.round(seconds / DAY);
  return days === 1 ? 'vor 1 Tag' : `vor ${days} Tagen`;
}

/** Formats a span in seconds as "4 Std 12 Min", for uptime. */
export function duration(seconds: number): string {
  if (seconds < MINUTE) return `${Math.max(0, Math.round(seconds))} Sek`;
  if (seconds < HOUR) return `${Math.floor(seconds / MINUTE)} Min`;

  const hours = Math.floor(seconds / HOUR);
  const minutes = Math.floor((seconds % HOUR) / MINUTE);
  if (hours < 24) return `${hours} Std ${minutes} Min`;

  const days = Math.floor(hours / 24);
  return `${days} Tage ${hours % 24} Std`;
}

/** A full timestamp for the title attribute, where the exact value matters. */
export function exactTime(iso: string): string {
  const value = new Date(iso);
  return Number.isNaN(value.getTime()) ? iso : value.toLocaleString('de-DE');
}

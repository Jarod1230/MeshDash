/**
 * Warnungen: was der Dienst meldet, und wie es dasteht.
 *
 * Der Dienst warnt nur für Knoten, die jemand ausdrücklich beobachtet, und
 * für den getrennten eigenen Node — ADR-0021. Hier liegt nur, was beides
 * betrifft: die Formen, der Text dazu und die Frage „gilt das für diesen
 * Knoten". Das Holen und Zuhören steht in `useAlerts`.
 */

import { relativeTime } from './time';

/** Eine Warnung, wie `/api/v1/alerts/log` sie liefert. */
export interface Alert {
  readonly id: number;
  /** `silent` oder `disconnected`. */
  readonly kind: string;
  /** Schlüssel des beobachteten Knotens; leer für den eigenen Node. */
  readonly subject: string;
  /** Seit wann die Lage besteht — der letzte Advert, der Moment der Trennung. */
  readonly since: string;
  /** Wann die Frist abgelaufen war. */
  readonly raised_at: string;
  /** Wann es vorbei war; `null`, solange die Warnung gilt. */
  readonly cleared_at: string | null;
}

/** Ein beobachteter Knoten, wie `/api/v1/alerts/watched` ihn liefert. */
export interface Watched {
  readonly public_key: string;
  readonly added_at: string;
  readonly last_heard_at: string | null;
}

/** Ein Knoten wird still. */
export const SILENT = 'silent';
/** Der eigene Node ist nicht verbunden. */
export const DISCONNECTED = 'disconnected';

/** Nur, was gerade gilt. */
export function open(alerts: readonly Alert[]): Alert[] {
  return alerts.filter((alert) => alert.cleared_at === null);
}

/** Die geltende Warnung zu einem Knoten, falls es eine gibt. */
export function forNode(alerts: readonly Alert[], key: string): Alert | null {
  return open(alerts).find((alert) => alert.subject === key) ?? null;
}

/**
 * Ein Satz zu einer Warnung.
 *
 * Nennt **seit wann**, nicht wann gewarnt wurde: Dass die Frist um 3 Uhr
 * ablief, hilft niemandem; dass der Knoten seit Dienstagabend schweigt,
 * schon. `name` ist der Knotenname, wo es einen gibt; `everHeard` sagt, ob
 * `since` ein letztes Lebenszeichen ist oder der Beginn der Beobachtung.
 */
export function describe(
  alert: Alert,
  name: string | null,
  now: Date,
  everHeard = true,
): string {
  const since = relativeTime(alert.since, now);
  const who = name ?? 'Ein beobachteter Knoten';

  if (alert.kind === DISCONNECTED) {
    return `Kein eigener Node verbunden, ${since}.`;
  }

  if (alert.kind === SILENT) {
    // Wurde er nie gehört, zählt die Frist ab dem Beginn der Beobachtung —
    // „zuletzt gehört" wäre dann schlicht falsch.
    return everHeard
      ? `${who} ist still — zuletzt gehört ${since}.`
      : `${who} war noch nie zu hören, beobachtet ${since}.`;
  }

  // Eine Art, die diese Fassung nicht kennt, ist kein Grund, die Warnung zu
  // verschlucken: Der Dienst hält sie für wichtig genug.
  return `${name ?? 'Etwas'} meldet „${alert.kind}“, ${since}.`;
}

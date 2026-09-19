/**
 * Die Warnungen des Dienstes, laufend gehalten.
 *
 * Holt Verlauf und Beobachtungsliste, hört auf die Ereignisse des Moduls
 * `alerts` und meldet eine neu aufgezogene Warnung dem Browser, wenn der
 * Betreiber das erlaubt hat. Mehr Zustellwege gibt es nicht — ADR-0021.
 */

import { useCallback, useEffect, useRef } from 'react';
import { apiSend } from './api';
import { useLiveEvent } from './events';
import { useResource } from './useResource';
import { describe, open, type Alert, type Watched } from './alerts';

/** Was die Karte und die Knotentafel davon brauchen. */
export interface Alerts {
  /** Nur die geltenden Warnungen, neueste zuerst. */
  readonly open: readonly Alert[];
  /** Schlüssel aller beobachteten Knoten. */
  readonly watched: ReadonlySet<string>;
  /** Ob von diesem beobachteten Knoten je ein Advert kam. */
  readonly everHeard: (key: string) => boolean;
  /** Beobachten an- oder abschalten; wirft bei einem Fehlschlag. */
  readonly setWatched: (key: string, on: boolean) => Promise<void>;
}

/**
 * @param nameOf Wie ein Schlüssel heißt, für die Benachrichtigung. Der Hook
 *   kennt die Knoten nicht; wer ihn benutzt, schon.
 */
export function useAlerts(nameOf: (key: string) => string | null): Alerts {
  const log = useResource<Alert[]>('/alerts/log?limit=100');
  const watched = useResource<Watched[]>('/alerts/watched');

  // In einer Ref, weil der Zuhörer nur einmal aufgesetzt wird und sonst auf
  // den Stand von damals zeigt. Gesetzt im Effekt, nicht im Rendern: Ein
  // Rendern, das verworfen wird, darf nichts hinterlassen.
  const nameRef = useRef(nameOf);
  useEffect(() => {
    nameRef.current = nameOf;
  });

  useLiveEvent(
    (event) => event.type === 'module' && event.module === 'alerts',
    (event) => {
      log.reload();
      watched.reload();

      if (event.kind !== 'raised') return;
      const alert = event.data as Alert | undefined;
      if (alert === undefined) return;

      // Beim Aufziehen zählt nur, was in der Warnung steht; ob je ein Advert
      // kam, ist hier nicht zur Hand und ändert den Satz nur in einem Fall.
      notify(describe(alert, nameRef.current(alert.subject), new Date()));
    },
  );

  const setWatched = useCallback(
    async (key: string, on: boolean) => {
      await apiSend(on ? 'PUT' : 'DELETE', `/alerts/watched/${key}`);
      watched.reload();
      log.reload();
    },
    [watched, log],
  );

  const entries = list(watched.data);

  return {
    // Eine Antwort in unerwarteter Form ist ein Fehler im Dienst, aber kein
    // Grund, die Karte mitzureißen: dann eben keine Warnungen.
    open: open(list(log.data)),
    watched: new Set(entries.map((one) => one.public_key)),
    everHeard: (key) =>
      entries.find((one) => one.public_key === key)?.last_heard_at != null,
    setWatched,
  };
}

/** Was als Liste ankam, oder nichts. */
function list<T>(data: T[] | null): T[] {
  return Array.isArray(data) ? data : [];
}

/**
 * Sagt es dem Browser, falls er darf.
 *
 * Ohne Erlaubnis passiert nichts und wird auch nicht danach gefragt: Ein
 * Fenster, das ungefragt nach Rechten fragt, wird weggeklickt. Gefragt wird
 * auf der Einstellungsseite, wo die Frage erklärt werden kann.
 */
export function notify(text: string): void {
  try {
    if (typeof Notification === 'undefined' || Notification.permission !== 'granted') return;
    new Notification('MeshDash', { body: text });
  } catch {
    // Manche Browser werfen in einem nicht sicheren Kontext. Eine Warnung, die
    // nicht angezeigt werden kann, darf die Karte nicht mitreißen.
  }
}

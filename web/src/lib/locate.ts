import { resolve, type Named } from './prefix';

/**
 * Wo ein Knoten liegen **muss**, wenn er schon nicht sagt, wo er liegt.
 *
 * # Keine geschätzten Punkte
 *
 * Aus Empfangsstärke eine Entfernung zu rechnen setzt einen bekannten
 * Pfadverlust voraus. Bei LoRa hängt der an Gelände, Antennenhöhe und
 * Sendeleistung der Gegenstelle — alles unbekannt. Am eigenen Mesh kam
 * derselbe Knoten mit −7 dBm und mit −24 dBm an, bei praktisch gleichem SNR.
 *
 * Was ohne jedes Signalmodell wahr ist: **Hören zwei einander, sind sie in
 * Reichweite voneinander.** Mehr sagt eine Hörbeziehung nicht, und weniger
 * auch nicht. Daraus folgt ein Bereich, kein Ort — siehe ADR-0019.
 *
 * Alles hier ist reine Rechnung: keine Anfragen, kein Browser, prüfbar ohne
 * beides.
 */

/** Ein Knoten mit Ort, wie ihn diese Rechnung braucht. */
export interface Placed extends Named {
  readonly latitude: number;
  readonly longitude: number;
}

/** Eine beobachtete Hörbeziehung, wie `/api/v1/traffic/links` sie liefert. */
export interface Hearing {
  readonly talker: string;
  readonly listener: string;
  readonly heard: number;
}

/** Ein verorteter Nachbar und was ihn zum Anker macht. */
export interface Anchor {
  readonly node: Placed;
  /** Wie viele Pakete die Beziehung belegen — beide Richtungen zusammen. */
  readonly heard: number;
}

/** Der Bereich, in dem ein Knoten liegen muss. */
export interface Region {
  /** Schlüssel des eingegrenzten Knotens. */
  readonly key: string;
  /** Die verorteten Nachbarn, die ihn eingrenzen. */
  readonly anchors: readonly Anchor[];
  /**
   * Radius je Anker, in Metern — für alle gleich.
   *
   * Keine Konstante, sondern die größte Entfernung zwischen zwei verorteten
   * Knoten, die einander nachweislich hören. Eine Beobachtung aus genau
   * diesem Mesh statt einer Annahme über LoRa.
   */
  readonly reach: number;
}

/** Metres per degree of latitude; der Äquatorwert reicht über eine Region. */
const METRES_PER_DEGREE = 111_320;

/**
 * Entfernung zweier Orte in Metern.
 *
 * Längengrade mit dem Kosinus der mittleren Breite gestaucht — dieselbe lokale
 * Näherung wie in `ground/projection.ts`, und über die Spanne eines Mesh
 * genauer als jede globale Projektion.
 */
export function distance(a: Placed, b: Placed): number {
  const meanLatitude = ((a.latitude + b.latitude) / 2) * (Math.PI / 180);
  const north = (a.latitude - b.latitude) * METRES_PER_DEGREE;
  const east = (a.longitude - b.longitude) * METRES_PER_DEGREE * Math.cos(meanLatitude);

  return Math.hypot(north, east);
}

/**
 * Wie weit dieses Mesh nachweislich trägt.
 *
 * Die größte Entfernung zwischen zwei verorteten Knoten, die einander hören.
 * `null`, solange keine zwei verorteten Knoten eine Hörbeziehung haben — dann
 * gibt es nichts zu messen und folglich auch nichts einzugrenzen.
 */
export function reach(placed: readonly Placed[], hearings: readonly Hearing[]): number | null {
  let furthest: number | null = null;

  for (const hearing of hearings) {
    const talker = find(hearing.talker, placed);
    const listener = find(hearing.listener, placed);
    if (talker === null || listener === null || talker.key === listener.key) continue;

    const span = distance(talker, listener);
    if (furthest === null || span > furthest) furthest = span;
  }

  return furthest;
}

/**
 * Grenzt jeden Knoten ein, der keine Position meldet.
 *
 * Ein Knoten ohne verorteten Nachbarn kommt nicht vor: Über ihn ist nichts
 * bekannt, und ein Bereich über der halben Karte wäre keine Aussage, sondern
 * eine Verzierung.
 */
export function regions(
  unplaced: readonly Named[],
  placed: readonly Placed[],
  hearings: readonly Hearing[],
): Region[] {
  const span = reach(placed, hearings);
  if (span === null) return [];

  const anchors = new Map<string, Map<string, number>>();

  for (const hearing of hearings) {
    // Beide Richtungen zählen gleich: Wer wen hört, ändert nichts daran, dass
    // sie in Reichweite voneinander sind.
    for (const [oneSide, otherSide] of [
      [hearing.talker, hearing.listener],
      [hearing.listener, hearing.talker],
    ] as const) {
      const node = find(oneSide, unplaced);
      const anchor = find(otherSide, placed);
      if (node === null || anchor === null) continue;

      const forNode = anchors.get(node.key) ?? new Map<string, number>();
      forNode.set(anchor.key, (forNode.get(anchor.key) ?? 0) + hearing.heard);
      anchors.set(node.key, forNode);
    }
  }

  return [...anchors.entries()].map(([key, byAnchor]) => ({
    key,
    reach: span,
    anchors: [...byAnchor.entries()]
      .map(([anchorKey, heard]) => ({
        node: placed.find((one) => one.key === anchorKey)!,
        heard,
      }))
      // Der bestbelegte Anker zuerst — er trägt die Aussage.
      .sort((one, other) => other.heard - one.heard),
  }));
}

/**
 * Wie eng ein Bereich ist, in Metern: der Durchmesser dessen, was übrig bleibt.
 *
 * Ein Anker lässt eine Scheibe vom vollen Durchmesser. Jeder weitere schneidet
 * etwas weg, und wie viel, hängt daran, wie weit die Anker auseinanderliegen —
 * zwei Anker nebeneinander schneiden fast nichts, zwei weit auseinander viel.
 *
 * Eine **obere Schranke**, keine Fläche: Die genaue Schnittmenge zu berechnen
 * wäre mehr Rechnung, als die Aussage trägt. Wer den Bereich sieht, sieht ihn
 * ohnehin als Fläche; diese Zahl ist dafür da, ihn zu beschreiben.
 */
export function tightness(region: Region): number {
  const spans = region.anchors.map(() => region.reach * 2);

  for (const one of region.anchors) {
    for (const other of region.anchors) {
      if (one.node.key === other.node.key) continue;
      const apart = distance(one.node, other.node);
      // Zwei Kreise gleichen Radius' schneiden sich in einer Linse, die umso
      // schmaler wird, je weiter die Mittelpunkte auseinanderliegen.
      spans.push(Math.max(region.reach * 2 - apart, 0));
    }
  }

  return Math.min(...spans);
}

/**
 * Findet einen Knoten zu einem Präfix.
 *
 * Das **leere Präfix ist der eigene Node**: Wer ein Paket empfängt, steht nicht
 * im Pfad, den er empfangen hat, und hat dort folglich kein Präfix. Diesen Fall
 * zu übersehen hieße, den einen Anker zu verlieren, der immer verortet ist,
 * sobald der Betreiber ihn verortet hat — und damit fast jede Eingrenzung.
 */
function find<T extends Named>(prefix: string, nodes: readonly T[]): T | null {
  if (prefix === '') return nodes.find((node) => node.own) ?? null;

  const key = resolve(prefix, nodes);

  return key === null ? null : (nodes.find((node) => node.key === key) ?? null);
}

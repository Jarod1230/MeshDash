/**
 * What an LPP type code means, in words.
 *
 * The names and units come from `src/helpers/sensors/LPPDataHelpers.h`,
 * MeshCore commit d929643 — the same table the decoder in `meshdash_proto::lpp`
 * is built from. A code that is not listed is shown as its number rather than
 * guessed at: a wrong label on a real measurement is worse than no label.
 */
const NAMES: Readonly<Record<number, readonly [string, string]>> = {
  0: ['Digitaleingang', ''],
  1: ['Digitalausgang', ''],
  2: ['Analogeingang', ''],
  3: ['Analogausgang', ''],
  100: ['Zähler', ''],
  101: ['Helligkeit', 'lx'],
  102: ['Anwesenheit', ''],
  103: ['Temperatur', '°C'],
  104: ['Luftfeuchte', '%'],
  113: ['Beschleunigung', 'g'],
  115: ['Luftdruck', 'hPa'],
  116: ['Spannung', 'V'],
  117: ['Strom', 'A'],
  118: ['Frequenz', 'Hz'],
  120: ['Anteil', '%'],
  121: ['Höhe', 'm'],
  125: ['Konzentration', 'ppm'],
  128: ['Leistung', 'W'],
  130: ['Entfernung', 'm'],
  131: ['Energie', 'kWh'],
  132: ['Richtung', '°'],
  133: ['Zeitstempel', ''],
  134: ['Drehrate', '°/s'],
  135: ['Farbe', ''],
  136: ['Position', ''],
  142: ['Schalter', ''],
};

/** The name of a measurement, or its bare code when it is unknown here. */
export function typeName(code: number): string {
  return NAMES[code]?.[0] ?? `Typ ${code}`;
}

/** The unit, empty where the type has none. */
export function typeUnit(code: number): string {
  return NAMES[code]?.[1] ?? '';
}

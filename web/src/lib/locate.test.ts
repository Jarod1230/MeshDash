import { describe, expect, it } from 'vitest';
import { distance, reach, regions, tightness, type Hearing, type Placed } from './locate';
import type { Named } from './prefix';

const OWN = '99'.repeat(32);
const NORTH = 'aa' + '11'.repeat(31);
const EAST = 'bb' + '22'.repeat(31);
const UNKNOWN = 'cc' + '33'.repeat(31);

function placed(key: string, latitude: number, longitude: number, own = false): Placed {
  return { key, name: key.slice(0, 4), own, latitude, longitude };
}

/** Stralsund und Umgebung, damit die Zahlen eine Größe haben. */
const HOME = placed(OWN, 54.3104, 13.0813, true);
const BRIDGE = placed(NORTH, 54.3304, 13.0813); // gut 2 km nördlich
const FAR = placed(EAST, 54.3104, 13.1813); // gut 6 km östlich

const NOBODY: Named = { key: UNKNOWN, name: 'unbekannt', own: false };

function hearing(talker: string, listener: string, heard = 5): Hearing {
  return { talker, listener, heard };
}

describe('distance', () => {
  it('misst über die Spanne eines Mesh brauchbar genau', () => {
    // 0,02° Breite sind gut 2,2 km — nachrechenbar ohne Kartenwerk.
    expect(distance(HOME, BRIDGE)).toBeGreaterThan(2_100);
    expect(distance(HOME, BRIDGE)).toBeLessThan(2_300);
  });

  it('staucht Längengrade, statt sie wie Breitengrade zu behandeln', () => {
    // Auf 54° Nord ist ein Längengrad nur knapp 59 % eines Breitengrades.
    expect(distance(HOME, FAR)).toBeLessThan(distance(HOME, placed('x', 54.4104, 13.0813)));
  });
});

describe('reach', () => {
  it('misst die Reichweite, statt sie anzunehmen', () => {
    // Die größte belegte Strecke zwischen zwei verorteten Knoten.
    const span = reach([HOME, BRIDGE, FAR], [hearing(NORTH, ''), hearing(EAST, '')]);

    expect(span).toBeCloseTo(distance(HOME, FAR), 0);
  });

  it('kennt keine Reichweite, solange keine zwei Verorteten einander hören', () => {
    // Ohne Beobachtung keine Zahl — und ohne Zahl keine Eingrenzung.
    expect(reach([HOME, BRIDGE], [])).toBeNull();
    expect(reach([HOME], [hearing(UNKNOWN, '')])).toBeNull();
  });
});

describe('regions', () => {
  const HEARINGS = [
    hearing(NORTH, ''), // Brücke wird vom eigenen Node gehört
    hearing(EAST, ''), // ebenso der ferne
    hearing(UNKNOWN, '', 12), // und der unbekannte
    hearing(UNKNOWN, 'aa', 3), // den außerdem die Brücke hört
  ];

  it('grenzt einen unverorteten Knoten auf seine verorteten Nachbarn ein', () => {
    const found = regions([NOBODY], [HOME, BRIDGE, FAR], HEARINGS);

    expect(found).toHaveLength(1);
    expect(found[0]?.anchors.map((one) => one.node.key)).toEqual([OWN, NORTH]);
    // Der bestbelegte Anker zuerst: zwölf Pakete gegen drei.
    expect(found[0]?.anchors[0]?.heard).toBe(12);
  });

  it('nimmt den eigenen Node als Anker, obwohl er kein Präfix hat', () => {
    // Wer empfängt, steht nicht im Pfad, den er empfängt. Diesen Fall zu
    // übersehen kostet fast jede Eingrenzung.
    // Eine Beziehung zwischen zwei Verorteten muss dabei sein, sonst gibt es
    // keine gemessene Reichweite und folglich nichts einzugrenzen.
    const found = regions(
      [NOBODY],
      [HOME, BRIDGE, FAR],
      [hearing(EAST, ''), hearing(UNKNOWN, '', 4)],
    );

    expect(found[0]?.anchors).toHaveLength(1);
    expect(found[0]?.anchors[0]?.node.key).toBe(OWN);
  });

  it('zählt beide Richtungen, weil Reichweite keine Richtung hat', () => {
    const found = regions([NOBODY], [HOME, BRIDGE, FAR], [...HEARINGS, hearing('', UNKNOWN, 7)]);

    expect(found[0]?.anchors[0]?.heard).toBe(19);
  });

  it('sagt nichts über einen Knoten ohne verorteten Nachbarn', () => {
    // Ein Bereich über der halben Karte ist keine Aussage, sondern Verzierung.
    const found = regions([NOBODY], [HOME, BRIDGE, FAR], [hearing(UNKNOWN, 'dd')]);

    expect(found).toEqual([]);
  });

  it('grenzt nichts ein, solange die Reichweite unbekannt ist', () => {
    expect(regions([NOBODY], [HOME], [hearing(UNKNOWN, '')])).toEqual([]);
  });

  it('benennt niemanden, wenn ein Präfix auf zwei Knoten passt', () => {
    const twins: Named[] = [
      { key: 'cc' + '44'.repeat(31), name: 'eins', own: false },
      { key: 'cc' + '55'.repeat(31), name: 'zwei', own: false },
    ];

    expect(regions(twins, [HOME, BRIDGE, FAR], HEARINGS)).toEqual([]);
  });
});

describe('tightness', () => {
  const span = distance(HOME, FAR);

  it('lässt einem einzelnen Anker die volle Scheibe', () => {
    const region = { key: UNKNOWN, reach: span, anchors: [{ node: HOME, heard: 3 }] };

    expect(tightness(region)).toBeCloseTo(span * 2, 0);
  });

  it('wird enger, je weiter zwei Anker auseinanderliegen', () => {
    const nearby = {
      key: UNKNOWN,
      reach: span,
      anchors: [
        { node: HOME, heard: 3 },
        { node: BRIDGE, heard: 3 },
      ],
    };
    const apart = {
      key: UNKNOWN,
      reach: span,
      anchors: [
        { node: HOME, heard: 3 },
        { node: FAR, heard: 3 },
      ],
    };

    expect(tightness(apart)).toBeLessThan(tightness(nearby));
    expect(tightness(nearby)).toBeLessThan(span * 2);
  });
});

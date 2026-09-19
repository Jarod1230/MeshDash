import { describe as group, expect, it } from 'vitest';
import { describe, forNode, open, DISCONNECTED, SILENT, type Alert } from './alerts';

const NOW = new Date('2026-09-19T20:00:00Z');

function alert(over: Partial<Alert> = {}): Alert {
  return {
    id: 1,
    kind: SILENT,
    subject: 'fb'.repeat(32),
    since: '2026-09-18T20:00:00Z',
    raised_at: '2026-09-19T19:00:00Z',
    cleared_at: null,
    ...over,
  };
}

group('open', () => {
  it('lässt weg, was vorbei ist', () => {
    const over = alert({ id: 2, cleared_at: '2026-09-19T19:30:00Z' });

    expect(open([alert(), over]).map((one) => one.id)).toEqual([1]);
  });
});

group('forNode', () => {
  it('findet die geltende Warnung zu einem Knoten', () => {
    expect(forNode([alert()], 'fb'.repeat(32))?.id).toBe(1);
    expect(forNode([alert()], 'aa'.repeat(32))).toBeNull();
  });

  it('meldet nichts zu einem Knoten, dessen Warnung vorbei ist', () => {
    expect(forNode([alert({ cleared_at: '2026-09-19T19:30:00Z' })], 'fb'.repeat(32))).toBeNull();
  });
});

group('describe', () => {
  it('nennt, seit wann es so ist — nicht, wann gewarnt wurde', () => {
    const text = describe(alert(), 'J-HomeNode', NOW);

    expect(text).toContain('J-HomeNode');
    expect(text).toContain('still');
    // 24 Stunden seit dem letzten Advert, nicht die eine Stunde seit der Warnung.
    expect(text).not.toContain('19:00');
  });

  it('braucht für den eigenen Node keinen Namen', () => {
    expect(describe(alert({ kind: DISCONNECTED, subject: '' }), null, NOW)).toContain(
      'Kein eigener Node verbunden',
    );
  });

  it('verschluckt eine unbekannte Art nicht', () => {
    expect(describe(alert({ kind: 'akku' }), 'X', NOW)).toContain('akku');
  });
});

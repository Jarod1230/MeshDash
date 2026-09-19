import { describe, expect, it } from 'vitest';
import { regionsNote } from './Ground';
import type { Region } from '../lib/locate';

const REGION: Region = {
  key: 'aa'.repeat(32),
  reach: 6_400,
  anchors: [],
};

describe('regionsNote', () => {
  it('names why nothing is drawn when no reach can be measured', () => {
    expect(regionsNote({ span: null, regions: [] })).toContain('Keine Reichweite messbar');
  });

  it('keeps the measured reach visible even when nobody can be bounded', () => {
    const note = regionsNote({ span: 6_400, regions: [] });

    expect(note).toContain('nachweisliche Reichweite');
    expect(note).toContain('kein Knoten ohne Position');
  });

  it('counts the bounded nodes', () => {
    expect(regionsNote({ span: 6_400, regions: [REGION, { ...REGION, key: 'bb'.repeat(32) }] })).toContain(
      '2 Knoten eingegrenzt',
    );
  });
});

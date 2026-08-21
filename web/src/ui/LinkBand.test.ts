import { describe, expect, it } from 'vitest';
import { toSegments, type Change } from './LinkBand';

describe('toSegments', () => {
  const now = new Date('2026-08-20T12:00:00Z').getTime();
  const at = (minutesAgo: number) => new Date(now - minutesAgo * 60_000).toISOString();

  it('turns changes into spans that run up to now', () => {
    // The API answers newest first; the band reads oldest to newest.
    const changes: Change[] = [
      { id: 3, at: at(10), connected: true, reason: null },
      { id: 2, at: at(12), connected: false, reason: 'Kabel gezogen' },
      { id: 1, at: at(60), connected: true, reason: null },
    ];

    const segments = toSegments(changes, now);

    expect(segments.map((s) => s.connected)).toEqual([true, false, true]);
    expect(segments[0]?.seconds).toBe(48 * 60);
    expect(segments[1]?.seconds).toBe(2 * 60);
    expect(segments[2]?.seconds).toBe(10 * 60);
    expect(segments[1]?.reason).toBe('Kabel gezogen');
  });

  it('has nothing to draw before the first connection', () => {
    expect(toSegments([], now)).toEqual([]);
  });

  it('skips an entry whose timestamp cannot be read', () => {
    const segments = toSegments([{ id: 1, at: 'kaputt', connected: true, reason: null }], now);
    expect(segments).toEqual([]);
  });
});

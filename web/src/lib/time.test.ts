import { describe, expect, it } from 'vitest';
import { duration, relativeTime } from './time';

describe('relativeTime', () => {
  const now = new Date('2026-08-20T12:00:00Z');
  const ago = (seconds: number) => new Date(now.getTime() - seconds * 1000).toISOString();

  it('reads recent moments as just now', () => {
    expect(relativeTime(ago(10), now)).toBe('gerade eben');
  });

  it('counts minutes, then hours, then days', () => {
    expect(relativeTime(ago(120), now)).toBe('vor 2 Min');
    expect(relativeTime(ago(3 * 3600 + 30 * 60), now)).toBe('vor 3 Std 30 Min');
    expect(relativeTime(ago(4 * 3600), now)).toBe('vor 4 Std');
    expect(relativeTime(ago(3 * 86400), now)).toBe('vor 3 Tagen');
  });

  it('does not count backwards when the node clock runs ahead', () => {
    // A node whose clock is a minute fast must not produce "vor -60 Min".
    expect(relativeTime(ago(-60), now)).toBe('gerade eben');
  });

  it('says so instead of printing Invalid Date', () => {
    expect(relativeTime('kein datum', now)).toBe('unbekannt');
  });
});

describe('duration', () => {
  it('scales from seconds to days', () => {
    expect(duration(30)).toBe('30 Sek');
    expect(duration(90)).toBe('1 Min');
    expect(duration(4 * 3600 + 12 * 60)).toBe('4 Std 12 Min');
    expect(duration(50 * 3600)).toBe('2 Tage 2 Std');
  });
});

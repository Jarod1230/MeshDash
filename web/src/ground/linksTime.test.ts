import { describe, expect, it } from 'vitest';
import {
  ABSPIELEN_PARAM,
  ZEIT_PARAM,
  clampReason,
  parseTrafficLinksResponse,
  readLinksTime,
  trafficLinksPath,
  writeLinksTime,
} from './linksTime';

describe('readLinksTime / writeLinksTime', () => {
  it('defaults to timeless with live playback', () => {
    expect(readLinksTime(new URLSearchParams())).toEqual({
      range: null,
      playback: 'jetzt',
    });
  });

  it('reads a duration and a playback offset from the address', () => {
    const params = new URLSearchParams(`${ZEIT_PARAM}=7d&${ABSPIELEN_PARAM}=7d`);
    expect(readLinksTime(params)).toEqual({ range: '7d', playback: '7d' });
  });

  it('ignores unknown values instead of inventing a window', () => {
    const params = new URLSearchParams(`${ZEIT_PARAM}=weird&${ABSPIELEN_PARAM}=nope`);
    expect(readLinksTime(params)).toEqual({ range: null, playback: 'jetzt' });
  });

  it('clears both keys when going timeless', () => {
    const params = new URLSearchParams(`knoten=aa&${ZEIT_PARAM}=24h&${ABSPIELEN_PARAM}=1d`);
    const next = writeLinksTime(params, { range: null, playback: 'jetzt' });
    expect(next.get('knoten')).toBe('aa');
    expect(next.has(ZEIT_PARAM)).toBe(false);
    expect(next.has(ABSPIELEN_PARAM)).toBe(false);
  });

  it('omits abspielen when playback is live', () => {
    const next = writeLinksTime(new URLSearchParams(), {
      range: '24h',
      playback: 'jetzt',
    });
    expect(next.get(ZEIT_PARAM)).toBe('24h');
    expect(next.has(ABSPIELEN_PARAM)).toBe(false);
  });
});

describe('trafficLinksPath', () => {
  const NOW = Date.parse('2026-09-06T12:00:30Z');

  it('asks for the timeless summary when no range is chosen', () => {
    expect(trafficLinksPath({ range: null, playback: 'jetzt' }, NOW)).toBe('/traffic/links');
  });

  it('floors to the minute and sends since and until', () => {
    // 12:00:30 → 12:00:00; last 24 hours ending now.
    expect(trafficLinksPath({ range: '24h', playback: 'jetzt' }, NOW)).toBe(
      `/traffic/links?since=${encodeURIComponent('2026-09-05T12:00:00.000Z')}` +
        `&until=${encodeURIComponent('2026-09-06T12:00:00.000Z')}`,
    );
  });

  it('shifts the whole window back for playback presets', () => {
    // Same 7-day span, ending one week earlier.
    expect(trafficLinksPath({ range: '7d', playback: '7d' }, NOW)).toBe(
      `/traffic/links?since=${encodeURIComponent('2026-08-23T12:00:00.000Z')}` +
        `&until=${encodeURIComponent('2026-08-30T12:00:00.000Z')}`,
    );
  });
});

describe('parseTrafficLinksResponse', () => {
  it('keeps a bare array as timeless', () => {
    const links = [
      {
        talker: 'aa',
        listener: 'bb',
        width: 1,
        first_seen: '2026-09-01T00:00:00Z',
        last_seen: '2026-09-02T00:00:00Z',
        heard: 2,
      },
    ];

    expect(parseTrafficLinksResponse(links, false)).toEqual({
      mode: 'timeless',
      links,
    });
  });

  it('unwraps the timed body and keeps the clamp signal', () => {
    const body = {
      clamped: true,
      keep_days: 30,
      effective_since: '2026-08-07T12:00:00+00:00',
      effective_until: '2026-09-06T12:00:00+00:00',
      links: [],
    };

    expect(parseTrafficLinksResponse(body, true)).toEqual({
      mode: 'timed',
      ...body,
    });
  });

  it('refuses a timed request that came back as an array', () => {
    expect(() => parseTrafficLinksResponse([], true)).toThrow(/wrap object/);
  });

  it('refuses a timeless request that came back as an object', () => {
    expect(() =>
      parseTrafficLinksResponse({ clamped: false, links: [] }, false),
    ).toThrow(/array/);
  });
});

describe('clampReason', () => {
  it('is silent when nothing was clamped', () => {
    expect(
      clampReason({
        mode: 'timed',
        clamped: false,
        keep_days: 30,
        effective_since: '2026-09-01T00:00:00Z',
        effective_until: '2026-09-06T00:00:00Z',
        links: [],
      }),
    ).toBeNull();
  });

  it('names the retention when the server shortened the window', () => {
    const text = clampReason({
      mode: 'timed',
      clamped: true,
      keep_days: 30,
      effective_since: '2026-08-07T12:00:00Z',
      effective_until: '2026-09-06T12:00:00Z',
      links: [],
    });

    expect(text).toMatch(/30 Tage/);
    expect(text).toMatch(/gekürzt/);
  });
});

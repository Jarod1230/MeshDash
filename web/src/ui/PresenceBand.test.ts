import { describe, expect, it } from 'vitest';
import { share } from './PresenceBand';

describe('share', () => {
  it('leaves silence at nothing', () => {
    expect(share(0, 10)).toBe(0);
  });

  it('lifts a single sighting clear of silence', () => {
    // One advert in a week of heavy traffic must not read as "not heard".
    expect(share(1, 500)).toBeGreaterThanOrEqual(35);
  });

  it('gives the busiest stretch the full weight', () => {
    expect(share(10, 10)).toBe(100);
  });
});

import { describe, expect, it } from 'vitest';
import { chunks } from './PacketList';

describe('chunks', () => {
  it('splits a path where the stations actually are', () => {
    // The width is the sender's choice, so the same bytes split differently.
    expect(chunks('aabb', 1)).toEqual(['aa', 'bb']);
    expect(chunks('aabb', 2)).toEqual(['aabb']);
    expect(chunks('aabbccddee', 2)).toEqual(['aabb', 'ccdd']);
  });

  it('drops a trailing remnant rather than inventing a station from it', () => {
    expect(chunks('aabbc', 2)).toEqual(['aabb']);
  });

  it('has nothing to split when the path is empty', () => {
    expect(chunks('', 1)).toEqual([]);
  });
});

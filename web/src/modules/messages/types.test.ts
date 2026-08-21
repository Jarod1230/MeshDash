import { describe, expect, it } from 'vitest';
import { conversationTitle, type Conversation } from './types';

const base: Conversation = {
  partner: 'contact',
  id: 'a1a1a1a1a1a1',
  name: null,
  candidates: 0,
  last_text: 'Hallo',
  last_at: new Date().toISOString(),
  last_direction: 'received',
  messages: 1,
};

describe('conversationTitle', () => {
  it('uses the name where one is known', () => {
    expect(conversationTitle({ ...base, name: 'Repeater Nord', candidates: 1 })).toBe(
      'Repeater Nord',
    );
  });

  it('falls back to the prefix for an unknown contact', () => {
    expect(conversationTitle(base)).toBe('a1a1a1a1a1a1');
  });

  it('says so when a prefix belongs to several contacts', () => {
    // Naming one of them would be a guess presented as fact.
    expect(conversationTitle({ ...base, candidates: 2 })).toBe('a1a1a1a1a1a1 — mehrdeutig');
  });

  it('names a channel by its index when it has no name', () => {
    expect(conversationTitle({ ...base, partner: 'channel', id: '2' })).toBe('Kanal 2');
  });
});

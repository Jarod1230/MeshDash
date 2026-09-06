import { describe, expect, it } from 'vitest';
import {
  channelThreads,
  conversationsForArea,
  findThread,
  matchingThreads,
  parseArea,
  partnerForArea,
} from './threads';
import type { Channel, Conversation } from './types';

const contact: Conversation = {
  partner: 'contact',
  id: 'a1a1a1a1a1a1',
  name: 'Repeater Nord',
  candidates: 1,
  public_key: 'aa'.repeat(32),
  last_text: 'Hallo aus dem Norden',
  last_at: '2026-09-06T10:00:00Z',
  last_direction: 'received',
  messages: 3,
};

const channel: Conversation = {
  partner: 'channel',
  id: '0',
  name: 'Public',
  candidates: 0,
  public_key: null,
  last_text: 'Wetter ok',
  last_at: '2026-09-06T11:00:00Z',
  last_direction: 'sent',
  messages: 2,
};

const ambiguous: Conversation = {
  ...contact,
  id: 'b2b2b2b2b2b2',
  name: null,
  candidates: 2,
  public_key: null,
  last_text: 'Anweisung',
};

describe('parseArea', () => {
  it('reads kanaele from the address', () => {
    expect(parseArea('kanaele')).toBe('kanaele');
  });

  it('falls back to direkt for anything else', () => {
    expect(parseArea(null)).toBe('direkt');
    expect(parseArea('gespraeche')).toBe('direkt');
  });
});

describe('partnerForArea', () => {
  it('maps the two areas to the API partner kinds', () => {
    expect(partnerForArea('direkt')).toBe('contact');
    expect(partnerForArea('kanaele')).toBe('channel');
  });
});

describe('conversationsForArea', () => {
  const all = [contact, channel, ambiguous];

  it('keeps only contacts under Direkt', () => {
    expect(conversationsForArea(all, 'direkt').map((c) => c.id)).toEqual([
      'a1a1a1a1a1a1',
      'b2b2b2b2b2b2',
    ]);
  });

  it('keeps only channels under Kanäle', () => {
    expect(conversationsForArea(all, 'kanaele').map((c) => c.id)).toEqual(['0']);
  });
});

describe('matchingThreads', () => {
  const all = [contact, channel, ambiguous];

  it('keeps everything when nothing was typed', () => {
    expect(matchingThreads(all, '   ')).toHaveLength(3);
  });

  it('finds a thread by part of its name', () => {
    expect(matchingThreads(all, 'nord').map((c) => c.id)).toEqual(['a1a1a1a1a1a1']);
  });

  it('finds a thread by last text', () => {
    expect(matchingThreads(all, 'wetter').map((c) => c.id)).toEqual(['0']);
  });

  it('finds an ambiguous contact by the mehrdeutig label', () => {
    expect(matchingThreads(all, 'mehrdeutig')).toHaveLength(1);
  });

  it('says nothing matched rather than falling back to everything', () => {
    expect(matchingThreads(all, 'Ostturm')).toHaveLength(0);
  });
});

describe('channelThreads', () => {
  const channels: Channel[] = [
    { channel_index: 0, name: 'Public', seen_at: '2026-09-01T00:00:00Z' },
    { channel_index: 2, name: 'Ops', seen_at: '2026-09-02T00:00:00Z' },
  ];

  it('keeps known channel threads and appends quiet ones', () => {
    const result = channelThreads([channel], channels);
    expect(result.map((c) => c.id)).toEqual(['0', '2']);
    expect(result[1]?.messages).toBe(0);
    expect(result[1]?.name).toBe('Ops');
  });

  it('does not duplicate a channel that already has a thread', () => {
    expect(channelThreads([channel], channels)).toHaveLength(2);
  });
});

describe('findThread', () => {
  it('returns the matching conversation', () => {
    expect(findThread([contact, channel], '0')?.partner).toBe('channel');
  });

  it('returns null when the address points nowhere', () => {
    expect(findThread([contact], 'missing')).toBeNull();
    expect(findThread([contact], null)).toBeNull();
  });
});

import { describe, expect, it } from 'vitest';
import { matching, matchingSightings } from './NodesPage';
import type { KnownContact } from './types';

const base: KnownContact = {
  public_key: 'aa'.repeat(32),
  name: 'Repeater Nord',
  contact_type: 2,
  flags: 0,
  position_source: null,
  path: null,
  stations: null,
  latitude: null,
  longitude: null,
  last_advert: 0,
  first_seen: '2026-08-20T10:00:00Z',
  last_seen: '2026-08-21T10:00:00Z',
};

const contacts: KnownContact[] = [
  base,
  { ...base, public_key: 'bc'.repeat(32), name: 'Handfunke Süd' },
];

describe('matching', () => {
  it('keeps everything when nothing was typed', () => {
    expect(matching(contacts, '   ')).toHaveLength(2);
  });

  it('finds a node by part of its name, whatever the case', () => {
    expect(matching(contacts, 'nord').map((c) => c.name)).toEqual(['Repeater Nord']);
  });

  it('finds a node by the prefix a message is filed under', () => {
    expect(matching(contacts, 'bcbcbc')).toHaveLength(1);
  });

  it('says nothing matched rather than falling back to everything', () => {
    expect(matching(contacts, 'Ostturm')).toHaveLength(0);
  });
});

const sightings = [
  { id: 1, public_key: 'aa'.repeat(32), heard_at: '2026-08-21T10:00:00Z', was_new: false },
  { id: 2, public_key: 'cd'.repeat(32), heard_at: '2026-08-21T10:05:00Z', was_new: true },
];

describe('matchingSightings', () => {
  it('keeps everything when nothing was typed', () => {
    expect(matchingSightings(sightings, contacts, '')).toHaveLength(2);
  });

  it('keeps a sighting whose contact matched', () => {
    expect(matchingSightings(sightings, matching(contacts, 'nord'), 'nord')).toHaveLength(1);
  });

  it('finds a node that transmits without being a contact', () => {
    // Nobody has a contact for cd…, which is exactly what makes it worth
    // finding by key.
    const found = matchingSightings(sightings, matching(contacts, 'cdcdcd'), 'cdcdcd');
    expect(found).toHaveLength(1);
    expect(found[0]?.public_key.startsWith('cd')).toBe(true);
  });
});

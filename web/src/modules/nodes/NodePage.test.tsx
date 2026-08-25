import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MemoryRouter, Route, Routes } from 'react-router-dom';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { NodePage } from './NodePage';
import type { KnownContact } from './types';

const KEY = 'aa'.repeat(32);

const contact: KnownContact = {
  public_key: KEY,
  name: 'Repeater Nord',
  contact_type: 2,
  flags: 0,
  position_source: null,
  reported_latitude: null,
  reported_longitude: null,
  path: '0102',
  stations: 2,
  latitude: 52.520008,
  longitude: 13.404954,
  last_advert: 1_700_000_000,
  first_seen: new Date(Date.now() - 86_400_000 * 3).toISOString(),
  last_seen: new Date(Date.now() - 120_000).toISOString(),
};

/** Answers each of the four endpoints the page asks. */
function answers(overrides: Record<string, unknown> = {}) {
  return vi.fn().mockImplementation((url: string) => {
    const path = String(url);
    const body =
      path.includes('/nodes/contacts')
        ? (overrides['contacts'] ?? [contact])
        : path.includes('/nodes/adverts')
          ? (overrides['adverts'] ?? [])
          : path.includes('/nodes/presence')
          ? (overrides['presence'] ?? { from: '', to: '', buckets: [] })
          : path.includes('/nodes/route-changes')
            ? (overrides['routeChanges'] ?? [])
            : path.includes('/telemetry/neighbours')
              ? (overrides['readings'] ?? [])
              : (overrides['thread'] ?? []);
    return Promise.resolve({ ok: true, status: 200, json: async () => body } as Response);
  });
}

function show(key = KEY) {
  render(
    <MemoryRouter initialEntries={[`/knoten/${key}`]}>
      <Routes>
        <Route path="/knoten/:key" element={<NodePage />} />
      </Routes>
    </MemoryRouter>,
  );
}

beforeEach(() => {
  vi.stubGlobal('matchMedia', (query: string) => ({
    matches: false,
    media: query,
    addEventListener: () => {},
    removeEventListener: () => {},
  }));
});

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('NodePage', () => {
  it('brings identity, route and position together', async () => {
    vi.stubGlobal('fetch', answers());
    show();

    expect(await screen.findByText('Repeater Nord')).toBeInTheDocument();
    expect(screen.getByText('2 Stationen')).toBeInTheDocument();
    expect(screen.getByText('52.52001, 13.40495')).toBeInTheDocument();
  });

  it('asks each endpoint for this node only', async () => {
    // A page about one node should not fetch everything and discard most.
    const fetchMock = answers();
    vi.stubGlobal('fetch', fetchMock);
    show();

    await screen.findByText('Repeater Nord');
    const urls = fetchMock.mock.calls.map((call) => String(call[0]));

    expect(urls.some((url) => url.includes(`/nodes/adverts?node=${KEY}`))).toBe(true);
    expect(urls.some((url) => url.includes(`/telemetry/neighbours?node=${KEY}`))).toBe(true);
    expect(urls.some((url) => url.includes('/messages/conversation?with=aaaaaaaaaaaa'))).toBe(true);
  });

  it('says so when the node is not known', async () => {
    vi.stubGlobal('fetch', answers({ contacts: [] }));
    show('ff'.repeat(32));

    expect(await screen.findByText('Dieser Knoten ist nicht bekannt.')).toBeInTheDocument();
  });

  it('marks a node that has been silent for over a day', async () => {
    const silent = { ...contact, last_seen: new Date(Date.now() - 86_400_000 * 2).toISOString() };
    vi.stubGlobal('fetch', answers({ contacts: [silent] }));
    show();

    expect(await screen.findByText(/schweigt seit über einem Tag/)).toBeInTheDocument();
  });

  it('explains an empty telemetry panel instead of leaving it blank', async () => {
    vi.stubGlobal('fetch', answers());
    show();

    expect(await screen.findByText(/nicht nach Messwerten gefragt/)).toBeInTheDocument();
  });
});

describe('Wegwechsel', () => {
  it('reads a change as a step from one route to another', async () => {
    vi.stubGlobal(
      'fetch',
      answers({
        routeChanges: [
          {
            id: 7,
            public_key: KEY,
            changed_at: new Date(Date.now() - 3_600_000).toISOString(),
            path: '010203',
            stations: 3,
            previous_path: '01',
            previous_stations: 1,
          },
        ],
      }),
    );
    show();

    expect(await screen.findByText('1 Station')).toBeInTheDocument();
    expect(screen.getByText('3 Stationen')).toBeInTheDocument();
    // The hop bytes themselves, for comparing two routes station by station.
    expect(screen.getByText('01 → 010203')).toBeInTheDocument();
  });

  it('says why an empty history is not a gap in the recording', async () => {
    vi.stubGlobal('fetch', answers());
    show();

    expect(await screen.findByText(/hat sich nicht geändert/)).toBeInTheDocument();
  });
});

describe('Erreichbarkeit', () => {
  it('shows the band over the stretches the API answered with', async () => {
    vi.stubGlobal(
      'fetch',
      answers({
        presence: {
          from: '2026-08-20T00:00:00Z',
          to: '2026-08-21T00:00:00Z',
          buckets: [
            { from: '2026-08-20T00:00:00Z', to: '2026-08-20T12:00:00Z', sightings: 4 },
            { from: '2026-08-20T12:00:00Z', to: '2026-08-21T00:00:00Z', sightings: 0 },
          ],
        },
      }),
    );
    show();

    const band = await screen.findByRole('img', { name: /Abschnitten des Zeitraums gehört/ });
    // Silence must be readable as silence, not as a hole in the recording.
    expect(band).toHaveAccessibleName('In 1 von 2 Abschnitten des Zeitraums gehört');
  });
});

describe('Position setzen', () => {
  it('offers to place a node that reports nothing', async () => {
    vi.stubGlobal('fetch', answers({ contacts: [{ ...contact, latitude: null, longitude: null }] }));
    show();

    expect(await screen.findByText(/steht er auf keiner Karte/)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Position setzen' })).toBeInTheDocument();
    // Nothing to take back until something was set.
    expect(screen.queryByRole('button', { name: 'Zurücknehmen' })).not.toBeInTheDocument();
  });

  it('sends what was typed and reloads', async () => {
    const fetchMock = answers({ contacts: [{ ...contact, latitude: null, longitude: null }] });
    vi.stubGlobal('fetch', fetchMock);
    show();

    await userEvent.type(await screen.findByLabelText('Breite'), '48.137');
    await userEvent.type(screen.getByLabelText('Länge'), '11.576');
    await userEvent.click(screen.getByRole('button', { name: 'Position setzen' }));

    const put = fetchMock.mock.calls.find(
      (call: unknown[]) => (call[1] as RequestInit | undefined)?.method === 'PUT',
    );
    expect(put).toBeDefined();
    expect(JSON.parse(String((put?.[1] as RequestInit).body))).toEqual({
      public_key: KEY,
      latitude: 48.137,
      longitude: 11.576,
    });
  });

  it('says what is wrong instead of sending nonsense', async () => {
    vi.stubGlobal('fetch', answers({ contacts: [{ ...contact, latitude: null, longitude: null }] }));
    show();

    await userEvent.type(await screen.findByLabelText('Breite'), 'Nordkamm');
    await userEvent.click(screen.getByRole('button', { name: 'Position setzen' }));

    expect(screen.getByRole('alert')).toHaveTextContent('Dezimalzahlen');
  });

  it('shows what the node claims beside a position that was set', async () => {
    vi.stubGlobal(
      'fetch',
      answers({
        contacts: [
          {
            ...contact,
            latitude: 48.137,
            longitude: 11.576,
            position_source: 'manual',
            reported_latitude: 52.520008,
            reported_longitude: 13.404954,
          },
        ],
      }),
    );
    show();

    // The node's own claim stays readable — a GPS in the wrong valley is
    // worth noticing.
    expect(await screen.findByText(/52.52001, 13.40495/)).toBeInTheDocument();
  });
});

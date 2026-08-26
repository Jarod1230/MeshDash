import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MemoryRouter } from 'react-router-dom';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { App } from './App';

/** A node that is up, with an identity to show. */
const STATUS = {
  connected: true,
  since: new Date(Date.now() - 4 * 3600 * 1000 - 12 * 60 * 1000).toISOString(),
  reason: null,
  node: {
    seen_at: new Date(Date.now() - 120 * 1000).toISOString(),
    firmware_version_code: 7,
    firmware_version: 'v1.7.2',
    manufacturer: 'Heltec V3',
    build_date: '2026-05-01',
    contact_capacity: 100,
    group_channels: 8,
    repeater_enabled: false,
  },
  node_self: {
    seen_at: new Date(Date.now() - 120 * 1000).toISOString(),
    public_key: 'ee'.repeat(32),
    name: 'DB0MSH',
    latitude: 52.520008,
    longitude: 13.404954,
    transmit_power_dbm: 22,
    max_power_dbm: 30,
    frequency_khz: 869_618,
    bandwidth_hz: 62_500,
    spreading_factor: 11,
    coding_rate: 5,
  },
};

/** Two seconds offline, six hours ago — enough for the band to have a notch. */
const HISTORY = [
  { id: 3, at: new Date(Date.now() - 4 * 3600 * 1000 - 12 * 60 * 1000).toISOString(), connected: true, reason: null },
  { id: 2, at: new Date(Date.now() - 6 * 3600 * 1000).toISOString(), connected: false, reason: 'Kabel gezogen' },
  { id: 1, at: new Date(Date.now() - 9 * 3600 * 1000).toISOString(), connected: true, reason: null },
];

/** Answers each API path with what that path actually returns. */
function answerWith(status: number, body: unknown) {
  return vi.fn().mockImplementation((url: string) => {
    const path = String(url);
    const answer =
      status !== 200
        ? body
        : path.includes('/system/connections')
          ? HISTORY
          : // The ground surface asks for these on every render of the shell.
            path.includes('/nodes/contacts')
            ? []
            : body;
    return Promise.resolve({
      ok: status >= 200 && status < 300,
      status,
      json: async () => answer,
    } as Response);
  });
}

/** Renders the shell at a path, since `/` is the map and shows no page. */
function show(path = '/verbindung') {
  render(
    <MemoryRouter initialEntries={[path]}>
      <App />
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

describe('App', () => {
  it('shows the link state and the node behind it', async () => {
    vi.stubGlobal('fetch', answerWith(200, STATUS));

    show();

    expect(await screen.findByText('Verbunden')).toBeInTheDocument();
    expect(screen.getByText('4 Std 12 Min')).toBeInTheDocument();
    expect(screen.getByText('v1.7.2')).toBeInTheDocument();
    expect(screen.getByText('Heltec V3')).toBeInTheDocument();
  });

  it('sends the stored token along', async () => {
    window.localStorage.setItem('meshdash.token', 'geheim');
    const fetchMock = answerWith(200, STATUS);
    vi.stubGlobal('fetch', fetchMock);

    show();

    await screen.findByText('Verbunden');
    const headers = (fetchMock.mock.calls[0]?.[1] as RequestInit).headers as Headers;
    expect(headers.get('Authorization')).toBe('Bearer geheim');
  });

  it('asks for a token only when the service rejects the request', async () => {
    vi.stubGlobal('fetch', answerWith(401, {}));

    show();

    expect(await screen.findByText('Token erforderlich')).toBeInTheDocument();
  });

  it('stays out of the way when no token is required', async () => {
    // The default install listens on loopback without a token (ADR-0006).
    // Showing a sign-in form there would invent a lock for an open door.
    vi.stubGlobal('fetch', answerWith(200, STATUS));

    show();

    await screen.findByText('Verbunden');
    expect(screen.queryByText('Token erforderlich')).not.toBeInTheDocument();
  });

  it('reports an unreachable service as such, not as a login problem', async () => {
    vi.stubGlobal('fetch', vi.fn().mockRejectedValue(new Error('Failed to fetch')));

    show();

    expect(await screen.findByRole('alert')).toHaveTextContent('antwortet nicht');
    expect(screen.queryByText('Token erforderlich')).not.toBeInTheDocument();
  });

  it('builds its navigation from the module registry', async () => {
    vi.stubGlobal('fetch', answerWith(200, STATUS));

    show();

    const nav = await screen.findByRole('navigation', { name: 'Hauptnavigation' });
    expect(nav).toBeInTheDocument();
    expect(screen.getByRole('link', { name: 'Verbindung' })).toBeInTheDocument();
  });

  it('remembers the chosen theme', async () => {
    vi.stubGlobal('fetch', answerWith(200, STATUS));

    show();

    await userEvent.click(await screen.findByRole('button', { name: /hellen Ansicht/ }));

    await waitFor(() => {
      expect(document.documentElement.dataset['theme']).toBe('light');
    });
    expect(window.localStorage.getItem('meshdash.theme')).toBe('light');
  });

  it('names the recorded dropouts, because a reconnect hides them', async () => {
    // Status alone says "connected" either way; the reason a link died is the
    // thing an operator is actually after.
    vi.stubGlobal('fetch', answerWith(200, STATUS));

    show();

    // The German framing is the interface's; the reason itself is a quoted
    // technical detail from the transport layer.
    expect(await screen.findByText('Verbindung abgerissen')).toBeInTheDocument();
    expect(screen.getByText('Kabel gezogen')).toBeInTheDocument();
    expect(screen.getByText('1 im geladenen Zeitraum')).toBeInTheDocument();
  });
});

describe('Dieser Node im Mesh', () => {
  it('shows what the node says about itself, in the units an operator reads', async () => {
    vi.stubGlobal('fetch', answerWith(200, STATUS));

    show();

    expect(await screen.findByText('DB0MSH')).toBeInTheDocument();
    // Kilohertz on the wire, megahertz on the dial.
    expect(screen.getByText('869.618 MHz')).toBeInTheDocument();
    expect(screen.getByText('62.5 kHz')).toBeInTheDocument();
    expect(screen.getByText('22 von 30 dBm')).toBeInTheDocument();
  });
});

describe('Die Karte als Grundfläche', () => {
  it('opens on the surface, with no page in front of it', async () => {
    vi.stubGlobal('fetch', answerWith(200, STATUS));
    show('/');

    // No positions in this fixture, so the honest arrangement is the rings.
    expect(await screen.findByRole('img', { name: /Netzansicht/ })).toBeInTheDocument();
    expect(screen.queryByRole('heading', { name: 'Verbindung' })).not.toBeInTheDocument();
  });

  it('draws a geography as soon as two nodes report where they are', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockImplementation((url: string) => {
        const body = String(url).includes('/nodes/contacts')
          ? [contact('a', 54.0, 13.0), contact('b', 54.02, 13.04)]
          : String(url).includes('/system/connections')
            ? HISTORY
            : STATUS;
        return Promise.resolve({ ok: true, status: 200, json: async () => body } as Response);
      }),
    );
    show('/');

    expect(await screen.findByRole('img', { name: /Karte mit/ })).toBeInTheDocument();
  });

  it('keeps the surface mounted while a page is open', async () => {
    vi.stubGlobal('fetch', answerWith(200, STATUS));
    show('/verbindung');

    // The point of the shutter: the drawing underneath is not thrown away,
    // so the section an operator was looking at survives the detour.
    expect(await screen.findByRole('heading', { name: 'Verbindung' })).toBeInTheDocument();
    expect(screen.getByRole('img', { name: /Netzansicht/ })).toBeInTheDocument();
  });

  it('closes the page on Escape and leaves the surface standing', async () => {
    vi.stubGlobal('fetch', answerWith(200, STATUS));
    show('/verbindung');

    await screen.findByRole('heading', { name: 'Verbindung' });
    await userEvent.keyboard('{Escape}');

    await waitFor(() =>
      expect(screen.queryByRole('heading', { name: 'Verbindung' })).not.toBeInTheDocument(),
    );
    expect(screen.getByRole('img', { name: /Netzansicht/ })).toBeInTheDocument();
  });
});

/** A contact that reports where it is. */
function contact(key: string, latitude: number, longitude: number) {
  return {
    public_key: key,
    name: `Knoten ${key}`,
    contact_type: 2,
    flags: 0,
    position_source: 'advert',
    path: '',
    stations: 0,
    latitude,
    longitude,
    last_advert: 1_700_000_000,
    first_seen: new Date().toISOString(),
    last_seen: new Date().toISOString(),
  };
}

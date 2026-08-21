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
    const isHistory = String(url).includes('/system/connections');
    return Promise.resolve({
      ok: status >= 200 && status < 300,
      status,
      json: async () => (isHistory && status === 200 ? HISTORY : body),
    } as Response);
  });
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

    render(
      <MemoryRouter>
        <App />
      </MemoryRouter>,
    );

    expect(await screen.findByText('Verbunden')).toBeInTheDocument();
    expect(screen.getByText('4 Std 12 Min')).toBeInTheDocument();
    expect(screen.getByText('v1.7.2')).toBeInTheDocument();
    expect(screen.getByText('Heltec V3')).toBeInTheDocument();
  });

  it('sends the stored token along', async () => {
    window.localStorage.setItem('meshdash.token', 'geheim');
    const fetchMock = answerWith(200, STATUS);
    vi.stubGlobal('fetch', fetchMock);

    render(
      <MemoryRouter>
        <App />
      </MemoryRouter>,
    );

    await screen.findByText('Verbunden');
    const headers = (fetchMock.mock.calls[0]?.[1] as RequestInit).headers as Headers;
    expect(headers.get('Authorization')).toBe('Bearer geheim');
  });

  it('asks for a token only when the service rejects the request', async () => {
    vi.stubGlobal('fetch', answerWith(401, {}));

    render(
      <MemoryRouter>
        <App />
      </MemoryRouter>,
    );

    expect(await screen.findByText('Token erforderlich')).toBeInTheDocument();
  });

  it('stays out of the way when no token is required', async () => {
    // The default install listens on loopback without a token (ADR-0006).
    // Showing a sign-in form there would invent a lock for an open door.
    vi.stubGlobal('fetch', answerWith(200, STATUS));

    render(
      <MemoryRouter>
        <App />
      </MemoryRouter>,
    );

    await screen.findByText('Verbunden');
    expect(screen.queryByText('Token erforderlich')).not.toBeInTheDocument();
  });

  it('reports an unreachable service as such, not as a login problem', async () => {
    vi.stubGlobal('fetch', vi.fn().mockRejectedValue(new Error('Failed to fetch')));

    render(
      <MemoryRouter>
        <App />
      </MemoryRouter>,
    );

    expect(await screen.findByRole('alert')).toHaveTextContent('antwortet nicht');
    expect(screen.queryByText('Token erforderlich')).not.toBeInTheDocument();
  });

  it('builds its navigation from the module registry', async () => {
    vi.stubGlobal('fetch', answerWith(200, STATUS));

    render(
      <MemoryRouter>
        <App />
      </MemoryRouter>,
    );

    const nav = await screen.findByRole('navigation', { name: 'Hauptnavigation' });
    expect(nav).toBeInTheDocument();
    expect(screen.getByRole('link', { name: 'Übersicht' })).toBeInTheDocument();
  });

  it('remembers the chosen theme', async () => {
    vi.stubGlobal('fetch', answerWith(200, STATUS));

    render(
      <MemoryRouter>
        <App />
      </MemoryRouter>,
    );

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

    render(
      <MemoryRouter>
        <App />
      </MemoryRouter>,
    );

    // The German framing is the interface's; the reason itself is a quoted
    // technical detail from the transport layer.
    expect(await screen.findByText('Verbindung abgerissen')).toBeInTheDocument();
    expect(screen.getByText('Kabel gezogen')).toBeInTheDocument();
    expect(screen.getByText('1 im geladenen Zeitraum')).toBeInTheDocument();
  });
});

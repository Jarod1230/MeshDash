import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { MemoryRouter } from 'react-router-dom';
import { EventStream } from '../../lib/events';
import { MessagesPage } from './MessagesPage';
import type { Conversation } from './types';

const contactThread: Conversation = {
  partner: 'contact',
  id: 'a1a1a1a1a1a1',
  name: 'Repeater Nord',
  candidates: 1,
  public_key: 'aa'.repeat(32),
  last_text: 'Hallo',
  last_at: '2026-09-06T10:00:00Z',
  last_direction: 'received',
  messages: 1,
};

const ambiguousThread: Conversation = {
  partner: 'contact',
  id: 'b2b2b2b2b2b2',
  name: null,
  candidates: 2,
  public_key: null,
  last_text: 'Anweisung',
  last_at: '2026-09-06T09:00:00Z',
  last_direction: 'received',
  messages: 1,
};

const channelThread: Conversation = {
  partner: 'channel',
  id: '0',
  name: 'Public',
  candidates: 0,
  public_key: null,
  last_text: 'Wetter',
  last_at: '2026-09-06T11:00:00Z',
  last_direction: 'sent',
  messages: 2,
};

function answerWith(conversations: Conversation[]) {
  return vi.fn().mockImplementation((url: string) => {
    const path = String(url);
    let body: unknown = [];
    if (path.includes('/messages/conversations')) body = conversations;
    else if (path.includes('/messages/channels')) {
      body = [{ channel_index: 0, name: 'Public', seen_at: '2026-09-01T00:00:00Z' }];
    } else if (path.includes('/messages/conversation')) {
      body = [
        {
          direction: 'received',
          text: 'Hallo',
          at: '2026-09-06T10:00:00Z',
          snr: 5,
          stations: null,
          flooded: null,
        },
      ];
    }
    return Promise.resolve({
      ok: true,
      status: 200,
      json: async () => body,
    } as Response);
  });
}

class FakeSocket {
  onopen: (() => void) | null = null;
  onmessage: (() => void) | null = null;
  onclose: (() => void) | null = null;
  onerror: (() => void) | null = null;
  constructor(public url: string) {}
  send() {}
  close() {}
}

beforeEach(() => {
  vi.stubGlobal('WebSocket', FakeSocket);
});

afterEach(() => {
  vi.unstubAllGlobals();
});

function renderPage(initial = '/nachrichten') {
  return render(
    <MemoryRouter initialEntries={[initial]}>
      <EventStream>
        <MessagesPage />
      </EventStream>
    </MemoryRouter>,
  );
}

describe('Nachrichtenseite areas', () => {
  it('shows Direkt threads and hides channel threads there', async () => {
    vi.stubGlobal('fetch', answerWith([contactThread, channelThread]));
    renderPage();

    expect(await screen.findByText('Repeater Nord')).toBeInTheDocument();
    expect(screen.queryByText('Public')).not.toBeInTheDocument();
  });

  it('shows Kanäle when that area is selected', async () => {
    vi.stubGlobal('fetch', answerWith([contactThread, channelThread]));
    renderPage('/nachrichten?bereich=kanaele');

    expect(await screen.findByText('Public')).toBeInTheDocument();
    expect(screen.queryByText('Repeater Nord')).not.toBeInTheDocument();
  });

  it('refuses to guess when a prefix is ambiguous', async () => {
    vi.stubGlobal('fetch', answerWith([ambiguousThread]));
    renderPage();

    expect(await screen.findByText(/mehrdeutig/)).toBeInTheDocument();
    expect(screen.queryByText('Repeater Nord')).not.toBeInTheDocument();
  });

  it('opens a thread with compose inside, not a global send form', async () => {
    vi.stubGlobal('fetch', answerWith([contactThread]));
    renderPage();

    expect(screen.queryByRole('heading', { name: 'Senden' })).not.toBeInTheDocument();

    await userEvent.click(await screen.findByText('Repeater Nord'));

    expect(await screen.findByLabelText('Nachricht')).toBeInTheDocument();
    expect(screen.getByText('← Alle Direktnachrichten')).toBeInTheDocument();
  });

  it('filters threads by the search box above the list', async () => {
    vi.stubGlobal('fetch', answerWith([contactThread, ambiguousThread]));
    renderPage();

    await screen.findByText('Repeater Nord');
    await userEvent.type(screen.getByLabelText('Fäden durchsuchen'), 'nord');

    // Debounce is 250ms in useDebounced; wait for the filter to apply.
    await waitFor(() => {
      expect(screen.getByText('Repeater Nord')).toBeInTheDocument();
      expect(screen.queryByText(/mehrdeutig/)).not.toBeInTheDocument();
    });
  });
});

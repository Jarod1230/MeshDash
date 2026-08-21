import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import type { DirectMessage } from './types';

// The Sender component is not exported; these tests go through the page's
// rendering of a message list instead, which is what a reader sees.
import { MessagesPage } from './MessagesPage';
import { MemoryRouter } from 'react-router-dom';
import { EventStream } from '../../lib/events';
import { vi, beforeEach, afterEach } from 'vitest';

const base: DirectMessage = {
  id: 1,
  sender_prefix: 'a1a1a1a1a1a1',
  sender_name: null,
  sender_candidates: 0,
  text: 'Testnachricht',
  text_type: 0,
  snr: 5.5,
  path_len: null,
  sent_at: 1_700_000_000,
  received_at: new Date().toISOString(),
};

function answerWith(messages: DirectMessage[]) {
  return vi.fn().mockImplementation((url: string) =>
    Promise.resolve({
      ok: true,
      status: 200,
      json: async () => (String(url).includes('/messages/received') ? messages : []),
    } as Response),
  );
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

function show() {
  render(
    <MemoryRouter>
      <EventStream>
        <MessagesPage />
      </EventStream>
    </MemoryRouter>,
  );
}

describe('who sent a message', () => {
  it('names the sender when exactly one contact matches', async () => {
    vi.stubGlobal('fetch', answerWith([{ ...base, sender_name: 'Repeater Nord', sender_candidates: 1 }]));
    show();

    expect(await screen.findByText('Repeater Nord')).toBeInTheDocument();
  });

  it('refuses to guess when a prefix is ambiguous', async () => {
    // Six bytes can collide. A guess presented as fact is worse than a hex
    // prefix, especially where messages carry instructions.
    vi.stubGlobal('fetch', answerWith([{ ...base, sender_candidates: 2 }]));
    show();

    expect(await screen.findByText(/mehrdeutig/)).toBeInTheDocument();
    expect(screen.queryByText('Repeater Nord')).not.toBeInTheDocument();
  });

  it('shows the bare prefix for an unknown sender', async () => {
    vi.stubGlobal('fetch', answerWith([base]));
    show();

    expect(await screen.findByText(/von a1a1a1a1a1a1/)).toBeInTheDocument();
  });
});

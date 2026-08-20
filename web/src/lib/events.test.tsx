import { render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { EventStream, useLiveReload, useStreamLive, type AppEvent } from './events';

/** A WebSocket that a test can drive. */
class FakeSocket {
  static last: FakeSocket | null = null;
  onopen: (() => void) | null = null;
  onmessage: ((event: { data: string }) => void) | null = null;
  onclose: (() => void) | null = null;
  onerror: (() => void) | null = null;
  sent: string[] = [];
  closed = false;

  constructor(public url: string) {
    FakeSocket.last = this;
  }

  send(data: string) {
    this.sent.push(data);
  }

  close() {
    this.closed = true;
    this.onclose?.();
  }
}

/** Counts reloads in the document title, so a test can await them. */
function Probe({ matches }: { readonly matches: (event: AppEvent) => boolean }) {
  const live = useStreamLive();
  useLiveReload(matches, () => {
    document.title = `reloads:${Number(document.title.split(':')[1] ?? 0) + 1}`;
  });

  return <span>{live ? 'live' : 'still'}</span>;
}

beforeEach(() => {
  document.title = 'reloads:0';
  vi.stubGlobal('WebSocket', FakeSocket);
});

afterEach(() => {
  vi.unstubAllGlobals();
  FakeSocket.last = null;
});

describe('EventStream', () => {
  it('sends the token as the first message', async () => {
    // A browser cannot set a header on a WebSocket; the server expects the
    // token as the opening message instead.
    window.localStorage.setItem('meshdash.token', 'geheim');

    render(
      <EventStream>
        <Probe matches={() => false} />
      </EventStream>,
    );

    await waitFor(() => expect(FakeSocket.last).not.toBeNull());
    FakeSocket.last?.onopen?.();

    expect(FakeSocket.last?.sent).toEqual(['geheim']);
  });

  it('reports being live only while the socket is open', async () => {
    render(
      <EventStream>
        <Probe matches={() => false} />
      </EventStream>,
    );

    expect(screen.getByText('still')).toBeInTheDocument();
    FakeSocket.last?.onopen?.();
    await waitFor(() => expect(screen.getByText('live')).toBeInTheDocument());
  });

  it('passes matching events to the page and skips the rest', async () => {
    render(
      <EventStream>
        <Probe matches={(event) => event.type === 'node_connected'} />
      </EventStream>,
    );

    FakeSocket.last?.onopen?.();
    FakeSocket.last?.onmessage?.({ data: JSON.stringify({ type: 'node_connected' }) });
    FakeSocket.last?.onmessage?.({ data: JSON.stringify({ type: 'push', payload: '80aa' }) });

    await waitFor(() => expect(document.title).toBe('reloads:1'));
  });

  it('survives a frame it cannot read', async () => {
    render(
      <EventStream>
        <Probe matches={() => true} />
      </EventStream>,
    );

    FakeSocket.last?.onopen?.();
    FakeSocket.last?.onmessage?.({ data: 'kein json' });

    // Still connected: one bad frame is not worth tearing the stream down.
    await waitFor(() => expect(screen.getByText('live')).toBeInTheDocument());
  });
});

import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from 'react';
import { readToken } from './token';

/**
 * The live event stream, as the pages see it.
 *
 * The backend sends what happened on the bus. Pages do not read those events
 * directly — they say "reload me when something like this arrives", because
 * what the browser needs is not the event but fresh data.
 *
 * # Why the stream is not the data
 *
 * The stream is a live view, not a log: the server subscribes after the
 * handshake, so anything that happened while connecting is not in it, and a
 * slow client is told it fell behind rather than being lied to. Rebuilding
 * state from events alone would therefore drift. Reloading on a nudge cannot.
 */
export interface AppEvent {
  readonly type: 'node_connected' | 'node_disconnected' | 'push' | 'module';
  readonly reason?: string;
  readonly payload?: string;
  readonly module?: string;
  readonly kind?: string;
  readonly data?: unknown;
}

type Listener = (event: AppEvent) => void;

interface Stream {
  /** Whether the browser currently holds the stream open. */
  readonly live: boolean;
  /** Registers a listener; returns the function that removes it again. */
  readonly subscribe: (listener: Listener) => () => void;
}

const StreamContext = createContext<Stream>({ live: false, subscribe: () => () => {} });

/** How long to wait before reconnecting, growing to a ceiling. */
const FIRST_RETRY_MS = 1_000;
const MAX_RETRY_MS = 30_000;

export function EventStream({ children }: { readonly children: ReactNode }) {
  const [live, setLive] = useState(false);
  const listeners = useRef(new Set<Listener>());

  useEffect(() => {
    let socket: WebSocket | null = null;
    let retryMs = FIRST_RETRY_MS;
    let timer: number | undefined;
    let stopped = false;

    const open = () => {
      if (stopped) return;

      const scheme = window.location.protocol === 'https:' ? 'wss' : 'ws';
      socket = new WebSocket(`${scheme}://${window.location.host}/api/v1/events`);

      socket.onopen = () => {
        // The token travels as the first message: a browser cannot set a
        // header on a WebSocket, and a query string would end up in logs.
        const token = readToken();
        if (token !== null) socket?.send(token);
        retryMs = FIRST_RETRY_MS;
        setLive(true);
      };

      socket.onmessage = (message) => {
        try {
          const event = JSON.parse(String(message.data)) as AppEvent;
          for (const listener of listeners.current) listener(event);
        } catch {
          // A frame we cannot read is not worth tearing the stream down for.
        }
      };

      socket.onclose = () => {
        setLive(false);
        if (stopped) return;
        // Backoff on loss, not on the attempt: a socket that opens and dies
        // at once would otherwise spin at full speed.
        timer = window.setTimeout(open, retryMs);
        retryMs = Math.min(retryMs * 2, MAX_RETRY_MS);
      };

      socket.onerror = () => socket?.close();
    };

    open();

    return () => {
      stopped = true;
      if (timer !== undefined) window.clearTimeout(timer);
      socket?.close();
    };
  }, []);

  const subscribe = useCallback((listener: Listener) => {
    listeners.current.add(listener);
    return () => {
      listeners.current.delete(listener);
    };
  }, []);

  const value = useMemo(() => ({ live, subscribe }), [live, subscribe]);

  return <StreamContext.Provider value={value}>{children}</StreamContext.Provider>;
}

/** Whether the live stream is currently connected. */
export function useStreamLive(): boolean {
  return useContext(StreamContext).live;
}

/**
 * Calls `onEvent` for every event that matters to this page.
 *
 * `matches` decides what matters. It is deliberately the page's business:
 * only the page knows that a `push` carrying an advert should reload its
 * contact list.
 */
export function useLiveReload(matches: (event: AppEvent) => boolean, onEvent: () => void): void {
  const { subscribe } = useContext(StreamContext);
  const matchRef = useRef(matches);
  const actionRef = useRef(onEvent);

  // Kept current in an effect rather than during render: a page passes fresh
  // closures on every pass, and the subscription must not be torn down and
  // rebuilt for each of them.
  useEffect(() => {
    matchRef.current = matches;
    actionRef.current = onEvent;
  });

  useEffect(
    () =>
      subscribe((event) => {
        if (matchRef.current(event)) actionRef.current();
      }),
    [subscribe],
  );
}

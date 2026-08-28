import { act, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { EventStream } from '../lib/events';
import { useFlights, type Flight } from './useFlights';
import type { GroundNode } from './projection';

/**
 * The seam between a packet arriving and a dot on the map.
 *
 * The pieces on either side are covered elsewhere: `follow` and `positionOf`
 * by their own tests, the event by the traffic module's. What is only visible
 * here is that they are actually joined — and that a flight goes away on the
 * wall clock rather than when the browser feels like drawing, which is what a
 * hidden tab exposed on a real mesh.
 */
class FakeSocket {
  static last: FakeSocket | null = null;
  onopen: (() => void) | null = null;
  onmessage: ((event: { data: string }) => void) | null = null;
  onclose: (() => void) | null = null;
  onerror: (() => void) | null = null;

  constructor() {
    FakeSocket.last = this;
  }

  send() {}
  close() {}

  /** Hands the app one event, the way the service would. */
  deliver(event: unknown) {
    this.onmessage?.({ data: JSON.stringify(event) });
  }
}

const OWN: GroundNode = {
  key: '99'.repeat(32),
  name: 'eigener',
  latitude: 54.0,
  longitude: 13.0,
  stations: 0,
  lastSeen: 0,
  own: true,
  source: 'advert',
};

const BRIDGE: GroundNode = { ...OWN, key: 'fb' + '07'.repeat(31), name: 'Brücke', own: false, latitude: 54.01, longitude: 13.01 };

/** Shows how many packets are in the air, so a test can read it off. */
function Watcher({ nodes }: { readonly nodes: readonly GroundNode[] }) {
  const { flights }: { readonly flights: readonly Flight[] } = useFlights(nodes);

  return <output>{flights.length}</output>;
}

beforeEach(() => {
  vi.stubGlobal('WebSocket', FakeSocket);
  vi.useFakeTimers({ shouldAdvanceTime: true });
});

afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
  FakeSocket.last = null;
});

describe('vom Ereignis zum Punkt', () => {
  function show(nodes: readonly GroundNode[]) {
    render(
      <EventStream>
        <Watcher nodes={nodes} />
      </EventStream>,
    );
    act(() => FakeSocket.last?.onopen?.());
  }

  function packet(stations: readonly string[]) {
    return {
      type: 'module',
      module: 'traffic',
      kind: 'packet',
      data: { payload_type: 2, route_type: 1, stations, width: 2, snr: 12, rssi: -9, size: 23 },
    };
  }

  it('puts a packet in the air and lets it arrive on its own', () => {
    show([OWN, BRIDGE]);

    act(() => FakeSocket.last?.deliver(packet(['fb07'])));
    expect(screen.getByRole('status').textContent).toBe('1');

    // A single leg is drawn over 700ms. Nothing is rendered here, so no frame
    // is ever painted — arrival has to come from the clock, not from drawing.
    act(() => void vi.advanceTimersByTime(1_200));
    expect(screen.getByRole('status').textContent).toBe('0');
  });

  it('ignores a packet whose path leads nowhere it can draw', () => {
    show([OWN, BRIDGE]);

    // No stations at all: heard straight from a sender who is named only
    // inside the encrypted payload.
    act(() => FakeSocket.last?.deliver(packet([])));
    // A prefix that fits no known node.
    act(() => FakeSocket.last?.deliver(packet(['5555'])));

    expect(screen.getByRole('status').textContent).toBe('0');
  });

  it('leaves events from other modules alone', () => {
    show([OWN, BRIDGE]);

    act(() =>
      FakeSocket.last?.deliver({
        type: 'module',
        module: 'telemetry',
        kind: 'position',
        data: { stations: ['fb07'] },
      }),
    );

    expect(screen.getByRole('status').textContent).toBe('0');
  });
});

import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MemoryRouter } from 'react-router-dom';
import { describe, expect, it, vi } from 'vitest';
import { NodePanel } from './NodePanel';
import type { GroundNode } from './projection';
import type { Alert } from '../lib/alerts';

const NODE: GroundNode = {
  key: 'fb'.repeat(32),
  name: 'J-FestlandBrücke-1',
  latitude: 54.331,
  longitude: 13.07,
  stations: 0,
  lastSeen: Date.parse('2026-09-19T19:00:00Z'),
  own: false,
  source: 'advert',
};

const NOW = Date.parse('2026-09-19T20:00:00Z');

function show(over: Partial<Parameters<typeof NodePanel>[0]> = {}) {
  const onWatch = vi.fn().mockResolvedValue(undefined);
  render(
    <MemoryRouter>
      <NodePanel
        node={NODE}
        now={NOW}
        onClose={() => {}}
        watched={false}
        onWatch={onWatch}
        alert={null}
        everHeard
        {...over}
      />
    </MemoryRouter>,
  );

  return onWatch;
}

describe('NodePanel', () => {
  it('bietet an, den Knoten zu beobachten', async () => {
    const onWatch = show();

    await userEvent.click(screen.getByRole('button', { name: 'Beobachten' }));

    expect(onWatch).toHaveBeenCalledWith(true);
  });

  it('bietet einem beobachteten Knoten das Gegenteil an', async () => {
    const onWatch = show({ watched: true });

    await userEvent.click(screen.getByRole('button', { name: 'Wird beobachtet' }));

    expect(onWatch).toHaveBeenCalledWith(false);
  });

  it('behauptet nichts, was der Dienst nicht angenommen hat', async () => {
    const onWatch = vi.fn().mockRejectedValue(new Error('nein'));
    render(
      <MemoryRouter>
        <NodePanel
          node={NODE}
          now={NOW}
          onClose={() => {}}
          watched={false}
          onWatch={onWatch}
          alert={null}
          everHeard
        />
      </MemoryRouter>,
    );

    await userEvent.click(screen.getByRole('button', { name: 'Beobachten' }));

    await waitFor(() => expect(screen.getByText(/Ging nicht/)).toBeTruthy());
    expect(screen.getByRole('button', { name: 'Beobachten' })).toBeTruthy();
  });

  it('sagt, seit wann ein Knoten still ist', () => {
    const alert: Alert = {
      id: 1,
      kind: 'silent',
      subject: NODE.key,
      since: '2026-09-18T20:00:00Z',
      raised_at: '2026-09-19T19:00:00Z',
      cleared_at: null,
    };
    show({ alert });

    expect(screen.getByText(/ist still/)).toBeTruthy();
  });

  it('sagt es anders, wenn der Knoten nie zu hören war', () => {
    const alert: Alert = {
      id: 2,
      kind: 'silent',
      subject: NODE.key,
      since: '2026-09-19T19:00:00Z',
      raised_at: '2026-09-19T19:30:00Z',
      cleared_at: null,
    };
    show({ alert, everHeard: false });

    expect(screen.getByText(/noch nie zu hören/)).toBeTruthy();
  });

  it('bietet dem eigenen Node kein Beobachten an', () => {
    // Er warnt über seine eigene Verbindung, nicht über sein Advert.
    show({ node: { ...NODE, own: true } });

    expect(screen.queryByRole('button', { name: 'Beobachten' })).toBeNull();
  });
});

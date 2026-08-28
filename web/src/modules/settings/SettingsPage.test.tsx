import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { SettingsPage } from './SettingsPage';

const ANSWER = {
  modules: [
    {
      module: 'telemetry',
      values: { neighbours: false, every_minutes: 30, silent_after_hours: 24 },
      changed: false,
    },
    { module: 'traffic', values: { record: true, keep_days: 30 }, changed: true },
  ],
};

function answering(overrides: { readonly onPut?: (body: unknown) => void } = {}) {
  return vi.fn().mockImplementation((_url: string, init?: RequestInit) => {
    if (init?.method === 'PUT') {
      overrides.onPut?.(JSON.parse(String(init.body)));
      return Promise.resolve({ ok: true, status: 200, json: async () => ANSWER.modules[0] });
    }
    return Promise.resolve({ ok: true, status: 200, json: async () => ANSWER });
  });
}

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('Einstellungen', () => {
  it('shows every option with what it costs, not just its name', async () => {
    vi.stubGlobal('fetch', answering());
    render(<SettingsPage />);

    expect(await screen.findByRole('switch', { name: 'Nachbarn fragen' })).toHaveAttribute(
      'aria-checked',
      'false',
    );
    // The sentence that matters: asking transmits.
    expect(screen.getByText(/belegt Sendezeit/)).toBeInTheDocument();
    expect(screen.getByLabelText(/Abstand zwischen zwei Anfragen/)).toHaveValue(30);
  });

  it('sends only the option that was touched', async () => {
    // Sending the whole section back would overwrite what somebody else
    // changed in the meantime, and reset anything this page does not know.
    const sent: unknown[] = [];
    vi.stubGlobal('fetch', answering({ onPut: (body) => sent.push(body) }));
    render(<SettingsPage />);

    await userEvent.click(await screen.findByRole('switch', { name: 'Nachbarn fragen' }));

    await waitFor(() => expect(sent).toEqual([{ neighbours: true }]));
  });

  it('says which sections were changed here rather than in the file', async () => {
    vi.stubGlobal('fetch', answering());
    render(<SettingsPage />);

    expect(await screen.findByTitle(/meshdash.toml steht noch etwas anderes/)).toBeInTheDocument();
  });

  it('names what it cannot change, instead of leaving a gap', async () => {
    vi.stubGlobal('fetch', answering());
    render(<SettingsPage />);

    expect(await screen.findByText(/wie der Dienst startet/)).toBeInTheDocument();
  });

  it('keeps a number out of range to itself', async () => {
    // The module would refuse it anyway; asking and being told no is noise.
    const sent: unknown[] = [];
    vi.stubGlobal('fetch', answering({ onPut: (body) => sent.push(body) }));
    render(<SettingsPage />);

    const field = await screen.findByLabelText(/Verlauf aufbewahren/);
    await userEvent.clear(field);
    await userEvent.type(field, '0');
    await userEvent.tab();

    expect(sent).toEqual([]);
  });
});

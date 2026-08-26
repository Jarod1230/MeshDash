import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { Announce } from './Announce';

function show(latitude: number | null = null, longitude: number | null = null) {
  const onChanged = vi.fn();
  render(<Announce latitude={latitude} longitude={longitude} onChanged={onChanged} />);
  return onChanged;
}

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('Announce', () => {
  it('sends the position in degrees and reports that it stays here for now', async () => {
    const fetched = vi.fn().mockResolvedValue({ ok: true, status: 204 } as Response);
    vi.stubGlobal('fetch', fetched);
    const onChanged = show();

    await userEvent.type(screen.getByLabelText('Breite'), '54.331026');
    await userEvent.type(screen.getByLabelText('Länge'), '13.070254');
    await userEvent.click(screen.getByRole('button', { name: 'Position setzen' }));

    await waitFor(() => expect(onChanged).toHaveBeenCalled());
    const [url, init] = fetched.mock.calls[0] as [string, RequestInit];
    expect(url).toContain('/system/position');
    expect(init.method).toBe('PUT');
    expect(JSON.parse(String(init.body))).toEqual({ latitude: 54.331026, longitude: 13.070254 });

    // Setting is not sending: the mesh hears nothing until an advert goes out.
    expect(screen.getByRole('status').textContent).toContain('nächsten Advert');
  });

  it('does not bother the node with something that is not a coordinate', async () => {
    const fetched = vi.fn();
    vi.stubGlobal('fetch', fetched);
    show();

    await userEvent.type(screen.getByLabelText('Breite'), '91');
    await userEvent.type(screen.getByLabelText('Länge'), '13');
    await userEvent.click(screen.getByRole('button', { name: 'Position setzen' }));

    expect(screen.getByRole('alert')).toBeInTheDocument();
    expect(fetched).not.toHaveBeenCalled();
  });

  it('keeps flooding apart from a word to the neighbours', async () => {
    const fetched = vi.fn().mockResolvedValue({ ok: true, status: 204 } as Response);
    vi.stubGlobal('fetch', fetched);
    show();

    await userEvent.click(screen.getByRole('button', { name: 'Durch das ganze Mesh' }));
    await waitFor(() => expect(fetched).toHaveBeenCalled());

    const [url, init] = fetched.mock.calls[0] as [string, RequestInit];
    expect(url).toContain('/nodes/advert');
    expect(JSON.parse(String(init.body))).toEqual({ flood: true });
  });

  it('starts from what the node reports, so a correction is a correction', async () => {
    show(54.331026, 13.070254);

    expect(screen.getByLabelText('Breite')).toHaveValue('54.331026');
  });
});

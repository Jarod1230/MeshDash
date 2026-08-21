import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { usePagedResource } from './usePagedResource';

interface Row {
  readonly id: number;
  readonly text: string;
}

/** A listing of 5 rows the fake API hands out two at a time. */
const ROWS: Row[] = [5, 4, 3, 2, 1].map((id) => ({ id, text: `Zeile ${id}` }));

const gefragt: string[] = [];

function answerWithPages() {
  return vi.fn().mockImplementation((url: string) => {
    const path = String(url);
    gefragt.push(path);
    const limit = Number(new URL(path, 'http://x').searchParams.get('limit'));
    const before = new URL(path, 'http://x').searchParams.get('before');
    const rest = before === null ? ROWS : ROWS.filter((row) => row.id < Number(before));
    return Promise.resolve({
      ok: true,
      status: 200,
      json: async () => rest.slice(0, limit),
    } as Response);
  });
}

function Listing() {
  const rows = usePagedResource<Row>('/rows', 2);
  return (
    <div>
      <ul>
        {(rows.items ?? []).map((row) => (
          <li key={row.id}>{row.text}</li>
        ))}
      </ul>
      {rows.hasMore && (
        <button type="button" onClick={rows.loadMore}>
          Ältere laden
        </button>
      )}
    </div>
  );
}

beforeEach(() => {
  gefragt.length = 0;
});

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('usePagedResource', () => {
  it('appends the next older page instead of replacing what is shown', async () => {
    vi.stubGlobal('fetch', answerWithPages());
    render(<Listing />);

    expect(await screen.findByText('Zeile 5')).toBeInTheDocument();
    expect(screen.queryByText('Zeile 3')).not.toBeInTheDocument();

    await userEvent.click(screen.getByRole('button', { name: 'Ältere laden' }));

    // The first page is still on screen, the second one below it.
    expect(await screen.findByText('Zeile 3')).toBeInTheDocument();
    expect(screen.getByText('Zeile 5')).toBeInTheDocument();
    // The cursor is the id of the last row shown, not an offset.
    expect(gefragt[1]).toContain('before=4');
  });

  it('stops offering more once a page comes back short', async () => {
    vi.stubGlobal('fetch', answerWithPages());
    render(<Listing />);
    await screen.findByText('Zeile 5');

    await userEvent.click(screen.getByRole('button', { name: 'Ältere laden' }));
    await screen.findByText('Zeile 3');
    await userEvent.click(screen.getByRole('button', { name: 'Ältere laden' }));
    await screen.findByText('Zeile 1');

    // Five rows in pages of two: the last page holds one, so there is no
    // further page to ask for.
    expect(screen.queryByRole('button', { name: 'Ältere laden' })).not.toBeInTheDocument();
  });
});

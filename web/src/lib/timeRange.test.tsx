import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { useTimeRange } from './timeRange';
import { RangePicker } from '../ui/RangePicker';

function Picker() {
  const range = useTimeRange();
  return (
    <div>
      <RangePicker range={range} label="Zeitraum" />
      <p data-testid="query">{range.query}</p>
    </div>
  );
}

beforeEach(() => {
  vi.useFakeTimers({ shouldAdvanceTime: true });
  vi.setSystemTime(new Date('2026-08-22T12:00:30Z'));
});

afterEach(() => {
  vi.useRealTimers();
});

describe('useTimeRange', () => {
  it('asks for the last 24 hours by default', () => {
    render(<Picker />);

    // Rounded down to the full minute, so the path does not change on every
    // render — 12:00:30 becomes 12:00:00.
    expect(screen.getByTestId('query')).toHaveTextContent(
      `&since=${encodeURIComponent('2026-08-21T12:00:00.000Z')}`,
    );
  });

  it('drops the bound entirely for "alles"', async () => {
    render(<Picker />);

    await userEvent.click(screen.getByRole('button', { name: 'alles' }));

    // An empty query, not `since=` with nothing behind it: the backend would
    // read that as a timestamp it cannot parse and answer 400.
    expect(screen.getByTestId('query')).toHaveTextContent('');
    expect(screen.getByRole('button', { name: 'alles' })).toHaveAttribute('aria-pressed', 'true');
  });

  it('moves the bound when another stretch is chosen', async () => {
    render(<Picker />);

    await userEvent.click(screen.getByRole('button', { name: '1 Std' }));

    expect(screen.getByTestId('query')).toHaveTextContent(
      `&since=${encodeURIComponent('2026-08-22T11:00:00.000Z')}`,
    );
  });
});

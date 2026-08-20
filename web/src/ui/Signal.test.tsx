import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { SignalBars, SignalValue } from './Signal';

describe('SignalValue', () => {
  it('marks a positive value with a sign', () => {
    // Without it, "5.5 dB" and "−5.5 dB" look alike in a scanned column.
    render(<SignalValue snr={5.5} />);
    expect(screen.getByText('+5.5 dB')).toBeInTheDocument();
  });

  it('shows a negative value as it is', () => {
    render(<SignalValue snr={-11} />);
    expect(screen.getByText('-11.0 dB')).toBeInTheDocument();
  });

  it('shows a dash where the node reported nothing', () => {
    // Not zero: a missing measurement is not a measurement of zero.
    render(<SignalValue snr={null} />);
    expect(screen.getByText('—')).toBeInTheDocument();
  });
});

describe('SignalBars', () => {
  it('describes the quality for anyone not looking at the bars', () => {
    render(<SignalBars snr={-3.5} />);
    expect(screen.getByRole('img', { name: 'Empfangsqualität -3.5 Dezibel' })).toBeInTheDocument();
  });
});

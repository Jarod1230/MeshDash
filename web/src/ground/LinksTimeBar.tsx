import type { ReactNode } from 'react';
import {
  LINK_RANGES,
  PLAYBACK_PRESETS,
  type LinkRangeKey,
  type LinksTimeChoice,
  type PlaybackKey,
} from './linksTime';

/**
 * Zeitraumwähler and Abspielen for the link layer.
 *
 * Lives on the map (ADR-0011), not on a page. Off means timeless `/links`;
 * choosing a duration turns on since/until. Playback presets shift the same
 * window into the past — no scrubber in v1 (ADR-0018).
 */
export function LinksTimeBar({
  choice,
  onChange,
}: {
  readonly choice: LinksTimeChoice;
  readonly onChange: (next: LinksTimeChoice) => void;
}) {
  const setRange = (range: LinkRangeKey | null) => {
    onChange({
      range,
      playback: range === null ? 'jetzt' : choice.playback,
    });
  };

  const setPlayback = (playback: PlaybackKey) => {
    if (choice.range === null) return;
    onChange({ range: choice.range, playback });
  };

  return (
    <div className="flex flex-col gap-1.5 rounded-md border border-mesh-border bg-mesh-surface/90 px-2.5 py-2 text-xs backdrop-blur">
      <div className="flex flex-wrap items-center gap-1" role="group" aria-label="Zeitraum der Verbindungen">
        <span className="mr-1 text-mesh-muted">Zeit</span>
        <Chip pressed={choice.range === null} onClick={() => setRange(null)}>
          zeitlos
        </Chip>
        {LINK_RANGES.map((option) => (
          <Chip
            key={option.key}
            pressed={choice.range === option.key}
            onClick={() => setRange(option.key)}
          >
            {option.label}
          </Chip>
        ))}
      </div>

      {choice.range !== null && (
        <div className="flex flex-wrap items-center gap-1" role="group" aria-label="Abspielen">
          <span className="mr-1 text-mesh-muted">Abspielen</span>
          {PLAYBACK_PRESETS.map((option) => (
            <Chip
              key={option.key}
              pressed={choice.playback === option.key}
              onClick={() => setPlayback(option.key)}
            >
              {option.label}
            </Chip>
          ))}
        </div>
      )}
    </div>
  );
}

function Chip({
  pressed,
  onClick,
  children,
}: {
  readonly pressed: boolean;
  readonly onClick: () => void;
  readonly children: ReactNode;
}) {
  return (
    <button
      type="button"
      aria-pressed={pressed}
      onClick={onClick}
      className={`rounded-md border px-2 py-0.5 focus-visible:outline focus-visible:outline-2 focus-visible:outline-mesh-accent ${
        pressed
          ? 'border-mesh-accent text-mesh-text'
          : 'border-mesh-border text-mesh-muted hover:text-mesh-text'
      }`}
    >
      {children}
    </button>
  );
}

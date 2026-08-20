/**
 * The link, shown the way it is heard: as a carrier or as silence.
 *
 * This is the only animated thing in the interface, so that motion always
 * means one thing — the link is alive. A flat line means it is not. The pulse
 * stops under `prefers-reduced-motion`, where the colour still carries it.
 */
export function Carrier({ connected }: { readonly connected: boolean }) {
  return (
    <span
      className="relative inline-block h-[2px] w-16 overflow-hidden rounded-full bg-mesh-border align-middle"
      role="img"
      aria-label={connected ? 'Verbindung steht' : 'Keine Verbindung'}
    >
      {connected ? (
        <span className="carrier-pulse absolute inset-y-0 w-1/2 bg-mesh-accent" />
      ) : (
        <span className="absolute inset-y-0 left-0 w-full bg-mesh-bad opacity-60" />
      )}
    </span>
  );
}

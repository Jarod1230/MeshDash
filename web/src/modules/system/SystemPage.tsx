import { Carrier } from '../../ui/Carrier';
import { LinkBand, type Change } from '../../ui/LinkBand';
import { Empty, Failed, Loading } from '../../ui/States';
import { useNow } from '../../lib/useNow';
import { useResource } from '../../lib/useResource';
import { duration, exactTime, relativeTime } from '../../lib/time';

/** What `/api/v1/system/status` answers. */
interface SystemStatus {
  readonly connected: boolean;
  readonly since: string | null;
  readonly reason: string | null;
  readonly node: NodeIdentity | null;
}

interface NodeIdentity {
  readonly seen_at: string;
  readonly firmware_version_code: number;
  readonly firmware_version: string;
  readonly manufacturer: string;
  readonly build_date: string;
  readonly contact_capacity: number;
  readonly group_channels: number;
  readonly repeater_enabled: boolean | null;
}

/**
 * Is the node there, and has it stayed there?
 *
 * The whole page is built around that one question, which is why the link
 * state gets the width, the largest figure and the band, while the node's
 * firmware — interesting once, then never again — is a single line of text.
 */
export function SystemPage() {
  // A ticking clock, so "vor 2 Min" keeps counting instead of freezing at
  // whatever it said when the page happened to render.
  const now = useNow();
  const status = useResource<SystemStatus>('/system/status');
  const history = useResource<Change[]>('/system/connections?limit=200');

  if (status.error !== null && status.data === null) {
    return (
      <div className="rounded-lg border border-mesh-border bg-mesh-surface">
        <Failed error={status.error} onRetry={status.reload} />
      </div>
    );
  }

  if (status.data === null) {
    return (
      <div className="rounded-lg border border-mesh-border bg-mesh-surface">
        <Loading what="Der Verbindungsstatus" />
      </div>
    );
  }

  const { connected, since, reason, node } = status.data;
  const held = since === null ? null : (now - new Date(since).getTime()) / 1000;
  const drops = (history.data ?? []).filter((change) => !change.connected);

  return (
    <div className="space-y-4">
      <section className="rounded-lg border border-mesh-border bg-mesh-surface p-5">
        <div className="flex flex-wrap items-end justify-between gap-x-6 gap-y-3">
          <div>
            <div className="flex items-center gap-2.5">
              <Carrier connected={connected} />
              <span
                className={`text-xs uppercase tracking-[0.16em] ${
                  connected ? 'text-mesh-accent' : 'text-mesh-bad'
                }`}
              >
                {connected ? 'Verbunden' : 'Getrennt'}
              </span>
            </div>
            <div className="tabular mt-2 text-4xl leading-none text-mesh-text">
              {held === null ? '—' : duration(held)}
            </div>
            <div className="mt-1.5 text-sm text-mesh-muted">
              {connected ? 'ohne Unterbrechung' : reason !== null ? reason : 'ohne Angabe'}
              {since !== null && (
                <span className="text-mesh-faint"> · seit {exactTime(since)}</span>
              )}
            </div>
          </div>

          {node !== null && (
            <dl className="flex flex-wrap gap-x-6 gap-y-2 text-sm">
              <Fact term="Gerät" value={node.manufacturer} />
              <Fact term="Firmware" value={node.firmware_version} />
              <Fact term="Kontaktplätze" value={String(node.contact_capacity)} />
              <Fact term="Kanäle" value={String(node.group_channels)} />
            </dl>
          )}
        </div>

        <div className="mt-5">
          {history.error !== null && history.data === null ? (
            <p className="text-xs text-mesh-faint">Der Verlauf konnte nicht geladen werden.</p>
          ) : (
            <LinkBand changes={history.data ?? []} now={now} />
          )}
        </div>
      </section>

      <section className="rounded-lg border border-mesh-border bg-mesh-surface">
        <header className="flex items-baseline gap-3 border-b border-mesh-border px-4 py-2.5">
          <h2 className="text-sm text-mesh-text">Abbrüche</h2>
          <span className="text-xs text-mesh-faint">
            {drops.length === 0
              ? 'keine aufgezeichnet'
              : drops.length > 8
                ? `${drops.length} aufgezeichnet, die letzten acht`
                : `${drops.length} aufgezeichnet`}
          </span>
        </header>

        {drops.length === 0 ? (
          <Empty>
            Seit Beginn der Aufzeichnung ist die Verbindung nicht abgerissen. Bricht sie ab, steht
            hier der Grund, den der Dienst festgehalten hat.
          </Empty>
        ) : (
          <ul className="divide-y divide-mesh-border text-sm">
            {drops.slice(0, 8).map((drop) => (
              <li key={drop.at} className="px-4 py-2.5">
                <div className="flex items-baseline justify-between gap-4">
                  <span className="text-mesh-text">Verbindung abgerissen</span>
                  <span className="tabular shrink-0 text-mesh-muted" title={exactTime(drop.at)}>
                    {relativeTime(drop.at, new Date(now))}
                  </span>
                </div>
                {drop.reason !== null && (
                  // The reason comes from the transport layer and is English,
                  // like every log line in this project. It is quoted on its
                  // own line as the technical detail it is, rather than
                  // dressed up as interface text — see ADR-0004.
                  <p className="tabular mt-0.5 truncate text-xs text-mesh-faint" title={drop.reason}>
                    {drop.reason}
                  </p>
                )}
              </li>
            ))}
          </ul>
        )}
      </section>

      {node === null && (
        <section className="rounded-lg border border-mesh-border bg-mesh-surface">
          <Empty>
            Der Node hat sich noch nicht vorgestellt. Sobald eine Verbindung steht, erscheinen hier
            Gerät, Firmware und Ausstattung.
          </Empty>
        </section>
      )}

      {node !== null && (
        <p className="text-xs text-mesh-faint">
          Node zuletzt gelesen {relativeTime(node.seen_at, new Date(now))} · Firmware {node.firmware_version} (
          {node.firmware_version_code}), gebaut {node.build_date} ·{' '}
          {node.repeater_enabled === null
            ? 'Repeater-Betrieb meldet diese Firmware nicht'
            : node.repeater_enabled
              ? 'betreibt zusätzlich einen Repeater'
              : 'kein Repeater-Betrieb'}
        </p>
      )}
    </div>
  );
}

function Fact({ term, value }: { readonly term: string; readonly value: string }) {
  return (
    <div>
      <dt className="text-xs uppercase tracking-wider text-mesh-faint">{term}</dt>
      <dd className="tabular mt-0.5 text-mesh-text">{value}</dd>
    </div>
  );
}

import { Carrier } from '../../ui/Carrier';
import { LinkBand, type Change } from '../../ui/LinkBand';
import { Empty, Failed, Loading } from '../../ui/States';
import { useNow } from '../../lib/useNow';
import { usePagedResource } from '../../lib/usePagedResource';
import { useResource } from '../../lib/useResource';
import { More } from '../../ui/More';
import { duration, exactTime, relativeTime } from '../../lib/time';

/** What `/api/v1/system/status` answers. */
interface SystemStatus {
  readonly connected: boolean;
  readonly since: string | null;
  readonly reason: string | null;
  readonly node: NodeIdentity | null;
  readonly node_self: SelfDescription | null;
}

/**
 * Who the node is in the mesh, as it says itself.
 *
 * Answered only at the start of a session, and by nothing else — this is the
 * one place the node's own key, name and position come from.
 */
interface SelfDescription {
  readonly seen_at: string;
  readonly public_key: string;
  readonly name: string;
  readonly latitude: number | null;
  readonly longitude: number | null;
  readonly transmit_power_dbm: number;
  readonly max_power_dbm: number;
  /** Kilohertz. The neighbouring bandwidth is in hertz — the firmware's doing. */
  readonly frequency_khz: number;
  readonly bandwidth_hz: number;
  readonly spreading_factor: number;
  readonly coding_rate: number;
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
/** How many connection changes one page of the history holds. */
const HISTORY_PAGE = 200;

export function SystemPage() {
  // A ticking clock, so "vor 2 Min" keeps counting instead of freezing at
  // whatever it said when the page happened to render.
  const now = useNow();
  const status = useResource<SystemStatus>('/system/status');
  const history = usePagedResource<Change>('/system/connections', HISTORY_PAGE);

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
  const drops = (history.items ?? []).filter((change) => !change.connected);

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
          {history.error !== null && history.items === null ? (
            <p className="text-xs text-mesh-faint">Der Verlauf konnte nicht geladen werden.</p>
          ) : (
            <LinkBand changes={history.items ?? []} now={now} />
          )}
        </div>
      </section>

      <section className="rounded-lg border border-mesh-border bg-mesh-surface">
        <header className="flex items-baseline gap-3 border-b border-mesh-border px-4 py-2.5">
          <h2 className="text-sm text-mesh-text">Abbrüche</h2>
          <span className="text-xs text-mesh-faint">
            {drops.length === 0
              ? 'keine im geladenen Zeitraum'
              : `${drops.length} im geladenen Zeitraum`}
          </span>
        </header>

        {drops.length === 0 ? (
          <Empty>
            Seit Beginn der Aufzeichnung ist die Verbindung nicht abgerissen. Bricht sie ab, steht
            hier der Grund, den der Dienst festgehalten hat.
          </Empty>
        ) : (
          <ul className="divide-y divide-mesh-border text-sm">
            {drops.map((drop) => (
              <li key={drop.id} className="px-4 py-2.5">
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
        {history.hasMore && (
          <More onClick={history.loadMore} loading={history.loadingMore} what="Verbindungswechsel" />
        )}
      </section>

      {status.data.node_self !== null && (
        <section className="rounded-lg border border-mesh-border bg-mesh-surface p-5">
          <div className="flex flex-wrap items-baseline gap-x-4 gap-y-1">
            <h2 className="text-sm text-mesh-text">Dieser Node im Mesh</h2>
            <span className="text-xs text-mesh-faint">wie er sich selbst vorstellt</span>
          </div>
          <p className="mt-1 text-lg text-mesh-text">{status.data.node_self.name}</p>
          <p className="tabular mt-0.5 text-xs break-all text-mesh-faint">
            {status.data.node_self.public_key}
          </p>

          <dl className="mt-4 flex flex-wrap gap-x-8 gap-y-3 text-sm">
            <Fact
              term="Position"
              value={
                status.data.node_self.latitude === null || status.data.node_self.longitude === null
                  ? 'meldet keine'
                  : `${status.data.node_self.latitude.toFixed(5)}, ${status.data.node_self.longitude.toFixed(5)}`
              }
            />
            <Fact
              term="Sendeleistung"
              value={`${status.data.node_self.transmit_power_dbm} von ${status.data.node_self.max_power_dbm} dBm`}
            />
            {/* Kilohertz and hertz side by side, as the firmware sends them.
                Both are shown in the unit an operator reads on the radio. */}
            <Fact
              term="Frequenz"
              value={`${(status.data.node_self.frequency_khz / 1000).toFixed(3)} MHz`}
            />
            <Fact
              term="Bandbreite"
              value={`${(status.data.node_self.bandwidth_hz / 1000).toFixed(1)} kHz`}
            />
            <Fact term="Spreizfaktor" value={String(status.data.node_self.spreading_factor)} />
            <Fact term="Coderate" value={`4/${status.data.node_self.coding_rate}`} />
          </dl>
        </section>
      )}

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

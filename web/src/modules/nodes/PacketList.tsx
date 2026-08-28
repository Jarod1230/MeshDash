import { usePagedResource } from '../../lib/usePagedResource';
import { Empty, Failed, Loading } from '../../ui/States';
import { More } from '../../ui/More';
import { exactTime, relativeTime } from '../../lib/time';
import { SignalValue } from '../../ui/Signal';

/**
 * Every packet this node touched, as it was heard.
 *
 * # What a match here is worth
 *
 * A packet names the stations it passed through by the first bytes of their
 * key — one to three, chosen by whoever sent it. A one-byte prefix is 256
 * values, so in a mesh of a few dozen nodes it is likelier than not that two
 * of them share one. The list therefore says how wide each match was rather
 * than presenting all of them as equally certain.
 */
interface HeardPacket {
  readonly id: number;
  readonly heard_at: string;
  readonly route_type: number;
  readonly payload_type: number;
  readonly version: number;
  readonly stations: number;
  readonly path: string;
  readonly path_width: number;
  readonly snr: number | null;
  readonly rssi: number | null;
  readonly size: number;
}

/**
 * Names for the route types, `src/Packet.h`, firmware `d929643`.
 *
 * Naming a known value is presentation; reading the bytes is the service's
 * job and stays there.
 */
const ROUTES: Record<number, string> = {
  0: 'geflutet, mit Transportcodes',
  1: 'geflutet',
  2: 'gerichtet',
  3: 'gerichtet, mit Transportcodes',
};

/** Names for the payload types, same source. */
const PAYLOADS: Record<number, string> = {
  0x00: 'Anfrage',
  0x01: 'Antwort',
  0x02: 'Textnachricht',
  0x03: 'Quittung',
  0x04: 'Advert',
  0x05: 'Kanalnachricht',
  0x06: 'Kanaldatagramm',
  0x07: 'anonyme Anfrage',
  0x08: 'zurückgegebener Weg',
  0x09: 'Wegmessung',
  0x0a: 'Teil einer Folge',
  0x0b: 'Steuerung',
  0x0f: 'eigenes Format',
};

/** How many packets one page holds. */
const PAGE = 50;

export function PacketList({
  publicKey,
  now,
}: {
  readonly publicKey: string;
  readonly now: number;
}) {
  const packets = usePagedResource<HeardPacket>(
    `/traffic/packets?station=${publicKey}`,
    PAGE,
  );

  if (packets.error !== null && packets.items === null) {
    return <Failed error={packets.error} onRetry={packets.reload} />;
  }

  if (packets.items === null) {
    return <Loading what="Die gehörten Pakete" />;
  }

  if (packets.items.length === 0) {
    return (
      <Empty>
        Kein gehörtes Paket führt über diesen Knoten. Ein Paket nennt nur die Stationen, die es
        weitergereicht haben — wer es abgeschickt hat, steht in der verschlüsselten Nutzlast und
        bleibt zu.
      </Empty>
    );
  }

  const weak = packets.items.filter((packet) => packet.path_width === 1).length;

  return (
    <>
      <ul className="divide-y divide-mesh-border text-sm">
        {packets.items.map((packet) => (
          <li key={packet.id} className="px-4 py-2.5">
            <div className="flex flex-wrap items-baseline justify-between gap-x-4 gap-y-1">
              <span className="text-mesh-text">
                {PAYLOADS[packet.payload_type] ?? `Typ ${packet.payload_type}`}
                <span className="ml-2 text-xs text-mesh-muted">
                  {ROUTES[packet.route_type] ?? `Route ${packet.route_type}`}
                </span>
              </span>
              <span
                className="tabular shrink-0 text-xs text-mesh-muted"
                title={exactTime(packet.heard_at)}
              >
                {relativeTime(packet.heard_at, new Date(now))}
              </span>
            </div>

            <div className="mt-1 flex flex-wrap items-center gap-x-3 gap-y-1 text-xs">
              <span className="tabular text-mesh-faint">
                {packet.stations === 0 ? (
                  'ohne Station'
                ) : (
                  <>
                    Weg{' '}
                    {chunks(packet.path, packet.path_width).map((station, index) => (
                      <span key={`${packet.id}-${index}`}>
                        {index > 0 && <span className="text-mesh-border"> → </span>}
                        <span
                          className={
                            publicKey.startsWith(station) ? 'text-mesh-accent' : undefined
                          }
                        >
                          {station}
                        </span>
                      </span>
                    ))}
                  </>
                )}
              </span>
              {packet.snr !== null && <SignalValue snr={packet.snr} />}
              {packet.rssi !== null && (
                <span className="tabular text-mesh-faint">{packet.rssi} dBm</span>
              )}
              <span className="tabular text-mesh-faint">{packet.size} B</span>
            </div>
          </li>
        ))}
      </ul>

      {weak > 0 && (
        <p className="border-t border-mesh-border px-4 py-2.5 text-xs text-mesh-faint">
          {weak === packets.items.length ? 'Alle' : `${weak} der`} hier gezeigten Pakete nennen ihre
          Stationen mit nur einem Byte. Das sind 256 Möglichkeiten — bei einigen Dutzend Knoten
          kann ein solcher Eintrag auch einen anderen meinen. Wie breit ein Absender seine
          Stationen schreibt, entscheidet er selbst.
        </p>
      )}

      {packets.hasMore && (
        <More onClick={packets.loadMore} loading={packets.loadingMore} what="Pakete" />
      )}
    </>
  );
}

/** Splits a path into its stations, as the packet wrote them. */
export function chunks(path: string, width: number): string[] {
  const size = Math.max(width, 1) * 2;
  const stations: string[] = [];

  for (let at = 0; at + size <= path.length; at += size) {
    stations.push(path.slice(at, at + size));
  }

  return stations;
}

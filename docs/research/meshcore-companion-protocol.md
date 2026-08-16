# MeshCore Companion-Protokoll — Recherchestand

**Stand: 2026-08-16.** Verifikationsstufen nach [`README.md`](README.md).

Dieser Stand stammt **ausschließlich aus veröffentlichter Dokumentation**
(Stufe `DOKU`). Nichts davon wurde an Hardware oder am Firmware-Quellcode
geprüft. Vor der Umsetzung von Schritt 2 der [Roadmap](../roadmap.md) ist
mindestens das Framing hochzustufen.

> **Warnung.** Die Quellen widersprechen sich beim Framing. Details unten und in
> [`../lessons-learned.md`](../lessons-learned.md). Wer hier Werte übernimmt,
> ohne sie zu prüfen, baut einen Decoder, der nicht synchronisiert.

## Überblick

Ein MeshCore-Companion-Node spricht mit einer Client-Anwendung über ein binäres
Protokoll. Es wird über drei Transporte gefahren — BLE, USB/Serial und
TCP/WiFi —, und **das Framing unterscheidet sich je Transport.**

Der Nutzinhalt ist in allen Fällen gleich aufgebaut:

```
[ opcode: u8 ][ payload: variabel ]
```

- Mehrbyte-Ganzzahlen: Little-Endian
- Zeichenketten: UTF-8
- Zeichenketten und Binärdaten am Frame-Ende tragen keine Längenangabe; sie
  laufen bis zum Frame-Ende. Das Framing muss die Länge also liefern.

## Framing

### BLE — `DOKU`

Kein eigenes Framing. Jeder Schreibvorgang bzw. jede Notification auf der
Characteristic **ist** genau ein Frame; die Grenzen kommen vom BLE-Link-Layer,
der auch die Integrität sichert. Kein Längenpräfix, keine Prüfsumme.

Hinweis aus der Doku: Die Standard-MTU von 23 Byte (20 Byte Nutzlast) reicht für
größere Kommandos nicht; es sollte eine MTU von 512 ausgehandelt werden.

### USB/Serial und TCP — `DOKU`, **widersprüchlich**

Ein Bytestrom kennt keine Frame-Grenzen, deshalb gibt es hier ein Präfix aus
Richtungs-Marker und Länge:

```
[ marker: u8 ][ len: u16 little-endian ][ payload: len Bytes ]
```

**Die Richtung der Marker ist unklar.** Aus derselben Recherche kamen zwei
einander widersprechende Aussagen:

| Aussage | App → Radio | Radio → App |
| --- | --- | --- |
| Variante A | `0x3C` (`<`, 60) | `0x3E` (`>`, 62) |
| Variante B | `0x3E` (`>`, 62) | `0x3C` (`<`, 60) |

Variante B ist die ausführlichere und plausiblere Darstellung — „ausgehender
Frame beginnt mit Byte 62 (`>`), eingehender mit Byte 60 (`<`)" —, aber
**plausibel ist nicht verifiziert.**

**Vor der Umsetzung zu klären**, auf einem dieser Wege:

1. Hexdump einer echten Verbindung (`HARDWARE`) — der schnellste Weg.
2. Serial-Implementierung im Firmware-Quellcode nachlesen (`SOURCE`).
3. Eine funktionierende Fremdimplementierung ansehen (`REFERENZ`) — etwa
   `meshcore_py` oder `meshcore_c`, siehe Quellen.

Weitere offene Punkte beim Serial-Framing:

- Zählt `len` nur die Payload oder Marker und Längenfeld mit?
- Gibt es eine Prüfsumme? (Nicht erwähnt — vermutlich nein, aber nicht belegt.)
- Wie synchronisiert man nach einem Fehler wieder auf? (Marker suchen genügt
  nicht sicher, weil `0x3C`/`0x3E` auch in Nutzdaten vorkommen.)

## Opcodes

Sammelstand, alle Stufe `DOKU`. Die Tabellen sind **unvollständig** — die
Referenzimplementierungen kennen deutlich mehr Kommandos. Deshalb ist ein
`Unknown(u8)`-Fallback für jeden Bereich Pflicht.

Richtungsunterscheidung: Antworten liegen unter `0x80`, Pushes ab `0x80`.

### Kommandos (App → Radio)

| Hex | Name | Payload |
| --- | --- | --- |
| `0x01` | `CMD_APP_START` | 7 Byte reserviert + App-Name (UTF-8, optional) |
| `0x03` | `CMD_SEND_CHANNEL_MESSAGE` | `0x00` + Kanal + Zeitstempel (u32) + Text |
| `0x06` | `CMD_SET_DEVICE_TIME` | Zeitstempel |
| `0x0A` | `CMD_SYNC_NEXT_MESSAGE` | — |
| `0x14` | `CMD_GET_BATTERY` | — |
| `0x16` | `CMD_DEVICE_QUERY` | Sub-Kommando (`0x03`) |
| `0x1F` | `CMD_GET_CHANNEL` | Kanalindex (0–7) |
| `0x20` | `CMD_SET_CHANNEL` | Index + Name (32 Byte) + Secret (16 Byte) |
| `0x3E` | `CMD_SEND_CHANNEL_DATA` | Kanal + `path_len` + `data_type` (2 B) + Daten |

> `CMD_APP_START` muss laut Doku das erste Kommando nach dem Verbindungsaufbau
> sein.
>
> **Achtung:** `0x3E` ist hier ein Kommando-Opcode und gleichzeitig ein
> Kandidat für den Serial-Richtungs-Marker. Das ist kein Widerspruch — die
> Bytes liegen auf verschiedenen Ebenen —, aber eine Fehlerquelle beim
> Debuggen von Hexdumps.

### Antworten (Radio → App)

| Hex | Name | Payload |
| --- | --- | --- |
| `0x00` | `RESP_CODE_OK` | optional 4 Byte LE |
| `0x01` | `RESP_CODE_ERROR` | 1 Byte Fehlercode |
| `0x05` | `RESP_CODE_SELF_INFO` | Identität + Funkkonfiguration (58+ Byte) |
| `0x06` | `RESP_CODE_MSG_SENT` | Route-Flag + Tag (4 B) + Timeout (4 B) |
| `0x07` | `RESP_CODE_CONTACT_MSG_RECV` | Pubkey (6 B) + Pfad + Typ + Zeit (4 B) + Text |
| `0x08` | `RESP_CODE_CHANNEL_MSG_RECV` | Kanal + Pfad + Typ + Zeit (4 B) + Text |
| `0x0A` | `RESP_CODE_NO_MORE_MSGS` | — |
| `0x0C` | `RESP_CODE_BATTERY` | Spannung (2 B) + belegt KB (4 B) + gesamt KB (4 B) |
| `0x0D` | `RESP_CODE_DEVICE_INFO` | FW-Version + Fähigkeiten + Build-/Modell-Strings |
| `0x10` | `RESP_CODE_CONTACT_MSG_RECV_V3` | SNR (1 B) + 2 B reserviert + wie `0x07` |
| `0x11` | `RESP_CODE_CHANNEL_MSG_RECV_V3` | SNR (1 B) + 2 B reserviert + wie `0x08` |
| `0x12` | `RESP_CODE_CHANNEL_INFO` | Index + Name (32 B) + Secret (16 B) |
| `0x1B` | `RESP_CODE_CHANNEL_DATA_RECV` | SNR + reserviert + Kanal + Typ + Länge + Daten |

Die V3-Varianten liefern zusätzlich SNR. Welche Variante ein Node schickt, hängt
von der Firmware-Version ab — **beide sind zu unterstützen.**

### Pushes (Radio → App, unaufgefordert)

| Hex | Name | Payload |
| --- | --- | --- |
| `0x80` | `PUSH_CODE_ADVERTISEMENT` | Advert-Paketdaten |
| `0x82` | `PUSH_CODE_ACK` | ACK-Code (6 Byte) |
| `0x83` | `PUSH_CODE_MSG_WAITING` | — (löst Abholen per `0x0A` aus) |
| `0x88` | `PUSH_CODE_LOG_DATA` | RF-Log, ignorierbar |

`PUSH_CODE_MSG_WAITING` ist der Kern des Empfangsablaufs: Der Node meldet nur,
dass etwas anliegt; der Client holt die Nachrichten einzeln mit
`CMD_SYNC_NEXT_MESSAGE` ab, bis `RESP_CODE_NO_MORE_MSGS` kommt.

### Fehlercodes (Payload von `RESP_CODE_ERROR`)

| Wert | Name | Bedeutung |
| --- | --- | --- |
| 1 | `ERR_CODE_UNSUPPORTED_CMD` | Kommando nicht implementiert |
| 2 | `ERR_CODE_NOT_FOUND` | Ziel nicht gefunden |
| 3 | `ERR_CODE_TABLE_FULL` | Tabelle/Warteschlange voll, später erneut |
| 4 | `ERR_CODE_BAD_STATE` | Gerät nicht im passenden Zustand |
| 5 | `ERR_CODE_FILE_IO_ERROR` | Dateisystemfehler |
| 6 | `ERR_CODE_ILLEGAL_ARG` | Argument ungültig |

## Weitere bekannte Fähigkeiten

Aus der Ereignisliste von `meshcore_py` lässt sich ableiten, dass das Protokoll
deutlich mehr kann, als oben steht — die zugehörigen Opcodes sind uns aber
**nicht bekannt**:

Kontaktverwaltung (Liste, Neuzugang, Löschen), Pfadverwaltung (`ADVERT_PATH`,
`PATH_UPDATE`, `PATH_RESPONSE`), Nachbarabfrage (`NEIGHBOURS_RESPONSE`),
Anmeldung an Repeatern (`LOGIN_SUCCESS`, `LOGIN_FAILED`), Telemetrie
(`TELEMETRY_RESPONSE`), Statistiken (`STATS_CORE`, `STATS_RADIO`,
`STATS_PACKETS`), Trace und RX-Log, Schlüsselverwaltung, ACL.

Für MeshDash besonders relevant: **Nachbarabfrage, Pfadverwaltung, Telemetrie
und Statistiken** — das sind die Daten, die ein Betreiber-Dashboard ausmachen.
Deren Opcodes zu ermitteln ist Teil von Schritt 2 der Roadmap.

## Offene Fragen

1. Richtung der Serial-Marker (`0x3C` / `0x3E`) — **blockiert Schritt 2**.
2. Zählweise des Längenfelds.
3. Gibt es eine Prüfsumme im Serial-Framing?
4. Verwendet TCP dasselbe Framing wie Serial?
5. Opcodes für Kontakte, Nachbarn, Pfade, Telemetrie und Statistiken.
6. Genaue Feldaufteilung von `RESP_CODE_SELF_INFO` (58+ Byte).
7. Format der Advert-Paketdaten in `PUSH_CODE_ADVERTISEMENT`.
8. Ab welcher Firmware-Version kommen die V3-Nachrichtenvarianten?

## Quellen

Alle abgerufen am 2026-08-16.

- [Companion Radio Protocol — MeshCore Wiki](https://github.com/meshcore-dev/MeshCore/wiki/Companion-Radio-Protocol)
- [`docs/companion_protocol.md` — meshcore-dev/MeshCore](https://github.com/meshcore-dev/MeshCore/blob/main/docs/companion_protocol.md)
- [Companion Protocol — docs.meshcore.io](https://docs.meshcore.io/companion_protocol/) (nur BLE)
- [Command Protocol — DeepWiki](https://deepwiki.com/meshcore-dev/MeshCore/4.1.2-command-protocol)
- [Serial and Network Interfaces — DeepWiki](https://deepwiki.com/ripplebiz/MeshCore/6.4-serial-and-network-interfaces)

Referenzimplementierungen — die **verlässlichste** Quelle, weil sie
nachweislich funktionieren:

- [`meshcore_py`](https://github.com/meshcore-dev/meshcore_py) — Python, offiziell
- [`meshcore-cli`](https://github.com/meshcore-dev/meshcore-cli) — CLI auf `meshcore_py`
- [`meshcore_c`](https://github.com/SH3D/meshcore_c) — C99, einzelne portable Datei
- [`meshcore-rs`](https://github.com/andrewdavidmackenzie/meshcore-rs) — Rust-Portierung

Beim Übernehmen aus diesen Projekten die Lizenz beachten: MeshDash steht unter
GPL-3.0-or-later. Erkenntnisse übernehmen ist unproblematisch, Code kopieren
nur bei kompatibler Lizenz und mit Herkunftsvermerk.

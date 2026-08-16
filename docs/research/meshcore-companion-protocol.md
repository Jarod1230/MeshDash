# MeshCore Companion-Protokoll — Recherchestand

**Stand: 2026-08-16.** Verifikationsstufen nach [`README.md`](README.md).

Das **Framing für Serial und TCP ist verifiziert** (Stufe `SOURCE`, bestätigt
durch `REFERENZ`) — siehe unten. Alles Übrige, insbesondere sämtliche Opcodes
und Payload-Aufteilungen, stammt weiterhin **ausschließlich aus
veröffentlichter Dokumentation** (Stufe `DOKU`) und ist ungeprüft.

> **Warnung.** Die veröffentlichte Dokumentation war beim Framing
> widersprüchlich, und die plausibler wirkende Variante war die falsche. Details
> in [`../lessons-learned.md`](../lessons-learned.md). Für die Opcode-Tabellen
> gilt die Warnung unverändert weiter: Wer daraus Werte ohne Prüfung übernimmt,
> schreibt still falsche Daten in die Datenbank.

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

### USB/Serial und TCP — `SOURCE` (bestätigt durch `REFERENZ`)

Ein Bytestrom kennt keine Frame-Grenzen, deshalb gibt es hier ein Präfix aus
Richtungs-Marker und Länge:

```
[ marker: u8 ][ len: u16 little-endian ][ payload: len Bytes ]
```

**Die Richtung der Marker ist geklärt — es gilt Variante A:**

| Richtung | Marker | Belegstelle |
| --- | --- | --- |
| App → Radio | `0x3C` (`<`, 60) | Firmware `checkRecvFrame()` sucht `'<'`; `meshcore_py` sendet `\x3c` |
| Radio → App | `0x3E` (`>`, 62) | Firmware `writeFrame()` schreibt `'>'`; `meshcore_py` sucht `\x3e` |

Die Firmware sagt es an einer Stelle wörtlich: `'<' is 0x3c which indicates a
frame sent from app to radio` (`SerialWifiInterface.cpp`).

**Für MeshDash gilt die App-Seite, also spiegelverkehrt zur Firmware:** wir
senden Frames mit `0x3C` und empfangen Frames mit `0x3E`.

Beantwortet sind damit auch die übrigen Framing-Fragen:

- **`len` zählt nur die Payload**, ohne Marker und ohne Längenfeld. Die Firmware
  schreibt `len` aus `writeFrame(src, len)` und danach genau `len` Bytes;
  `meshcore_py` bildet `\x3c + len(data) + data`.
- **Es gibt keine Prüfsumme.** Weder Sende- noch Empfangspfad berechnet oder
  prüft eine. Die Integrität liefert bei USB die CDC-Schicht, bei TCP der
  Transport selbst.
- **TCP verwendet dasselbe Framing wie Serial.** Kommentar in der Firmware:
  `use same header as serial interface so client can delimit frames`.

#### Rahmengröße

`MAX_FRAME_SIZE` ist in der Firmware **176** Byte
(`BaseSerialInterface.h`, Kommentar: `+4 for transport codes (region
scoping)`). Größere Frames werden beim Senden **verworfen** — `writeFrame()`
gibt `0` zurück — und beim Empfang je nach Transport abgeschnitten (Serial) oder
übersprungen (TCP).

`meshcore_py` verwirft empfangsseitig erst ab **300** Byte. Das ist kein
Widerspruch, sondern eine großzügigere Plausibilitätsschranke im Client. Für
MeshDash folgt daraus: **senden ≤ 176 Byte**, empfangsseitig toleranter puffern
und die eigene Obergrenze nicht enger ziehen als der Node sie kennt.

#### Resynchronisierung

Die beiden Firmware-Transporte verhalten sich nach einem Fehler unterschiedlich
— relevant, weil unser Decoder beides überstehen muss:

- **Serial** verwirft im Zustand `IDLE` jedes Byte, das nicht `'<'` ist, und
  beginnt beim nächsten Markerbyte neu. Eine Längenprüfung findet **nicht**
  statt. Ein Markerbyte in den Nutzdaten kann nach einem verlorenen Frame also
  zu einer Fehlsynchronisierung führen.
- **TCP** liest den 3-Byte-Kopf am Stück, prüft den Frame-Typ und überspringt
  gezielt `len` Bytes, wenn Typ oder Länge nicht passen.

`meshcore_py` ergänzt clientseitig eine Heuristik, die die Firmware nicht hat:
Eine angekündigte Länge > 300 gilt als ungültig, der Puffer wird verworfen und
die Suche nach dem Marker beginnt von vorn. Zusätzlich überspringt es führenden
Müll vor dem Marker mit der Begründung, manche Radios mischten Konsolenausgaben
auf dieselbe UART.

Für unseren Decoder heißt das: Markersuche allein genügt nicht. Eine
Längen-Plausibilitätsprüfung beim Resync ist belegte Praxis der offiziellen
Referenzimplementierung und gehört eingebaut.

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
> **Achtung:** `0x3E` ist hier ein Kommando-Opcode und zugleich der
> Richtungs-Marker für Frames vom Radio zur App. Das ist kein Widerspruch — die
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

1. Opcodes für Kontakte, Nachbarn, Pfade, Telemetrie und Statistiken.
2. Genaue Feldaufteilung von `RESP_CODE_SELF_INFO` (58+ Byte).
3. Format der Advert-Paketdaten in `PUSH_CODE_ADVERTISEMENT`.
4. Ab welcher Firmware-Version kommen die V3-Nachrichtenvarianten?
5. Sämtliche Opcode-Werte und Payload-Aufteilungen oben stehen weiterhin auf
   Stufe `DOKU`. Sie lassen sich auf demselben Weg wie das Framing hochstufen —
   `examples/companion_radio/MyMesh.cpp` in der Firmware ist der Ort, an dem die
   Kommandos ausgewertet werden.

**Erledigt am 2026-08-16** (Belege im Abschnitt „Framing"): Richtung der
Serial-Marker, Zählweise des Längenfelds, Prüfsumme, Framing bei TCP.
Damit ist die Vorbedingung für Schritt 2 der [Roadmap](../roadmap.md) erfüllt.

## Quellen

Alle abgerufen am 2026-08-16.

### Für das Framing — Stufe `SOURCE` und `REFERENZ`

Firmware `meshcore-dev/MeshCore`, Commit `d929643`:

- [`src/helpers/ArduinoSerialInterface.cpp`](https://github.com/meshcore-dev/MeshCore/blob/d92964352441e53b93e8667b802e04f6e072b39e/src/helpers/ArduinoSerialInterface.cpp)
  — Serial-Framing beider Richtungen, Empfangs-Zustandsautomat
- [`src/helpers/BaseSerialInterface.h`](https://github.com/meshcore-dev/MeshCore/blob/d92964352441e53b93e8667b802e04f6e072b39e/src/helpers/BaseSerialInterface.h)
  — `MAX_FRAME_SIZE 176`
- [`src/helpers/esp32/SerialWifiInterface.cpp`](https://github.com/meshcore-dev/MeshCore/blob/d92964352441e53b93e8667b802e04f6e072b39e/src/helpers/esp32/SerialWifiInterface.cpp)
  — TCP-Framing, Typprüfung, wörtliche Aussage zur Marker-Richtung

Referenzimplementierung `meshcore-dev/meshcore_py`, Commit `c487efb`:

- [`src/meshcore/serial_cx.py`](https://github.com/meshcore-dev/meshcore_py/blob/c487efbe187f4b000020afdfc0349c4cdf503c5a/src/meshcore/serial_cx.py)
  — App-Seite: sendet `\x3c`, empfängt `\x3e`, Längen-Plausibilität ab 300

### Für die Opcodes — Stufe `DOKU`

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

# MeshCore Companion-Protokoll — Recherchestand

**Stand: 2026-08-16.** Verifikationsstufen nach [`README.md`](README.md).

Verifiziert am Firmware-Quellcode (Stufe `SOURCE`) sind:

- das **Framing** für Serial und TCP, zusätzlich bestätigt durch `REFERENZ`,
- sämtliche **Opcode-Werte** für Kommandos, Antworten, Pushes und Fehlercodes.

Ungeprüft sind weiterhin die **Payload-Aufteilungen**, von einzelnen Ausnahmen
abgesehen, die unten ausdrücklich als belegt gekennzeichnet sind.

> **Warnung.** Die veröffentlichte Dokumentation war beim Framing
> widersprüchlich, und die plausibler wirkende Variante war die falsche. Details
> in [`../lessons-learned.md`](../lessons-learned.md). Sie war außerdem bei den
> Opcodes stark unvollständig und in mehreren Namen ungenau. Für alles, was hier
> nicht als `SOURCE` markiert ist, gilt deshalb unverändert: Wer Werte ohne
> Prüfung übernimmt, schreibt still falsche Daten in die Datenbank.

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

**Die Opcode-Werte stehen auf Stufe `SOURCE`** — abgelesen an den `#define`s am
Kopf von `examples/companion_radio/MyMesh.cpp`, MeshCore-Commit `d929643`,
Firmware `v1.17.1` (`FIRMWARE_VER_CODE` 13).

**Die Payload-Aufteilungen sind es nicht.** Sie stammen weiterhin aus der
Dokumentation (`DOKU`) und sind unten getrennt gekennzeichnet. Wer ein Feld
auspacken will, verifiziert es vorher einzeln in `handleCmdFrame()`.

Richtungsunterscheidung: Antworten liegen unter `0x80`, Pushes ab `0x80`.

Ein `Unknown(u8)`-Fallback bleibt trotz vollständiger Tabelle Pflicht: Die
Firmware entwickelt sich weiter, und ein Node mit neuerer Firmware darf uns
nicht zum Absturz bringen.

### Protokollversionen

Wichtiger Befund, der die frühere Annahme korrigiert: **Die App bestimmt, welche
Varianten sie bekommt.** In `CMD_DEVICE_QUERY` sendet sie im zweiten Byte die
Protokollversion, die sie versteht; die Firmware merkt sie sich als
`app_target_ver` und richtet ihre Antworten danach.

Das heißt konkret: Ob wir `RESP_CODE_CONTACT_MSG_RECV` oder die V3-Variante mit
SNR erhalten, hängt **nicht** von der Firmware ab, sondern davon, was wir selbst
angesagt haben. Wer SNR will, muss mindestens Version 3 melden.

Im Quelltext markierte Versionsschwellen: `v3+` (SNR-Varianten, Kontakt- und
Kanalzahlen in `RESP_CODE_DEVICE_INFO`), `v8+` (Flood-Scope, Control-Data,
Statistiken), `v9+` (Repeater-Flag in `RESP_CODE_DEVICE_INFO`).

### Kommandos (App → Radio) — Werte `SOURCE`

| Dez | Hex | Name |
| --- | --- | --- |
| 1 | `0x01` | `CMD_APP_START` |
| 2 | `0x02` | `CMD_SEND_TXT_MSG` |
| 3 | `0x03` | `CMD_SEND_CHANNEL_TXT_MSG` |
| 4 | `0x04` | `CMD_GET_CONTACTS` (optional `since` für effizienten Abgleich) |
| 5 | `0x05` | `CMD_GET_DEVICE_TIME` |
| 6 | `0x06` | `CMD_SET_DEVICE_TIME` |
| 7 | `0x07` | `CMD_SEND_SELF_ADVERT` |
| 8 | `0x08` | `CMD_SET_ADVERT_NAME` |
| 9 | `0x09` | `CMD_ADD_UPDATE_CONTACT` |
| 10 | `0x0A` | `CMD_SYNC_NEXT_MESSAGE` |
| 11 | `0x0B` | `CMD_SET_RADIO_PARAMS` |
| 12 | `0x0C` | `CMD_SET_RADIO_TX_POWER` |
| 13 | `0x0D` | `CMD_RESET_PATH` |
| 14 | `0x0E` | `CMD_SET_ADVERT_LATLON` |
| 15 | `0x0F` | `CMD_REMOVE_CONTACT` |
| 16 | `0x10` | `CMD_SHARE_CONTACT` |
| 17 | `0x11` | `CMD_EXPORT_CONTACT` |
| 18 | `0x12` | `CMD_IMPORT_CONTACT` |
| 19 | `0x13` | `CMD_REBOOT` |
| 20 | `0x14` | `CMD_GET_BATT_AND_STORAGE` (früher `CMD_GET_BATTERY_VOLTAGE`) |
| 21 | `0x15` | `CMD_SET_TUNING_PARAMS` |
| 22 | `0x16` | `CMD_DEVICE_QUERY` |
| 23 | `0x17` | `CMD_EXPORT_PRIVATE_KEY` |
| 24 | `0x18` | `CMD_IMPORT_PRIVATE_KEY` |
| 25 | `0x19` | `CMD_SEND_RAW_DATA` |
| 26 | `0x1A` | `CMD_SEND_LOGIN` |
| 27 | `0x1B` | `CMD_SEND_STATUS_REQ` |
| 28 | `0x1C` | `CMD_HAS_CONNECTION` |
| 29 | `0x1D` | `CMD_LOGOUT` |
| 30 | `0x1E` | `CMD_GET_CONTACT_BY_KEY` |
| 31 | `0x1F` | `CMD_GET_CHANNEL` |
| 32 | `0x20` | `CMD_SET_CHANNEL` |
| 33 | `0x21` | `CMD_SIGN_START` |
| 34 | `0x22` | `CMD_SIGN_DATA` |
| 35 | `0x23` | `CMD_SIGN_FINISH` |
| 36 | `0x24` | `CMD_SEND_TRACE_PATH` |
| 37 | `0x25` | `CMD_SET_DEVICE_PIN` |
| 38 | `0x26` | `CMD_SET_OTHER_PARAMS` |
| 39 | `0x27` | `CMD_SEND_TELEMETRY_REQ` (im Quelltext als ablösbar markiert) |
| 40 | `0x28` | `CMD_GET_CUSTOM_VARS` |
| 41 | `0x29` | `CMD_SET_CUSTOM_VAR` |
| 42 | `0x2A` | `CMD_GET_ADVERT_PATH` |
| 43 | `0x2B` | `CMD_GET_TUNING_PARAMS` |
| 50 | `0x32` | `CMD_SEND_BINARY_REQ` |
| 51 | `0x33` | `CMD_FACTORY_RESET` |
| 52 | `0x34` | `CMD_SEND_PATH_DISCOVERY_REQ` |
| 54 | `0x36` | `CMD_SET_FLOOD_SCOPE_KEY` (v8+) |
| 55 | `0x37` | `CMD_SEND_CONTROL_DATA` (v8+) |
| 56 | `0x38` | `CMD_GET_STATS` (v8+, zweites Byte ist der Statistiktyp) |
| 57 | `0x39` | `CMD_SEND_ANON_REQ` |
| 58 | `0x3A` | `CMD_SET_AUTOADD_CONFIG` |
| 59 | `0x3B` | `CMD_GET_AUTOADD_CONFIG` |
| 60 | `0x3C` | `CMD_GET_ALLOWED_REPEAT_FREQ` |
| 61 | `0x3D` | `CMD_SET_PATH_HASH_MODE` |
| 62 | `0x3E` | `CMD_SEND_CHANNEL_DATA` |
| 63 | `0x3F` | `CMD_SET_DEFAULT_FLOOD_SCOPE` |
| 64 | `0x40` | `CMD_GET_DEFAULT_FLOOD_SCOPE` |
| 65 | `0x41` | `CMD_SEND_RAW_PACKET` |

**Lücken sind echt, nicht übersehen:** 44–49 sind im Quelltext ausdrücklich für
mögliche WLAN-Operationen reserviert („parked"). Für **53** steht keine
Begründung — der Wert fehlt einfach zwischen 52 und 54.

Statistiktypen als zweites Byte von `CMD_GET_STATS`: `0` = `STATS_TYPE_CORE`,
`1` = `STATS_TYPE_RADIO`, `2` = `STATS_TYPE_PACKETS`.

> **Achtung, zwei Fallen beim Debuggen von Hexdumps:** `0x3E` ist der
> Kommando-Opcode `CMD_SEND_CHANNEL_DATA` **und** der Richtungs-Marker für
> Frames vom Radio zur App. `0x3C` ist `CMD_GET_ALLOWED_REPEAT_FREQ` **und** der
> Marker für Frames von der App zum Radio. Das ist kein Widerspruch — die Bytes
> liegen auf verschiedenen Ebenen —, aber es sieht im Dump verwirrend aus.

### Antworten (Radio → App) — Werte `SOURCE`

| Dez | Hex | Name |
| --- | --- | --- |
| 0 | `0x00` | `RESP_CODE_OK` |
| 1 | `0x01` | `RESP_CODE_ERR` |
| 2 | `0x02` | `RESP_CODE_CONTACTS_START` (erste Antwort auf `CMD_GET_CONTACTS`) |
| 3 | `0x03` | `RESP_CODE_CONTACT` (mehrfach) |
| 4 | `0x04` | `RESP_CODE_END_OF_CONTACTS` |
| 5 | `0x05` | `RESP_CODE_SELF_INFO` (Antwort auf `CMD_APP_START`) |
| 6 | `0x06` | `RESP_CODE_SENT` |
| 7 | `0x07` | `RESP_CODE_CONTACT_MSG_RECV` (nur bei App-Version < 3) |
| 8 | `0x08` | `RESP_CODE_CHANNEL_MSG_RECV` (nur bei App-Version < 3) |
| 9 | `0x09` | `RESP_CODE_CURR_TIME` |
| 10 | `0x0A` | `RESP_CODE_NO_MORE_MESSAGES` |
| 11 | `0x0B` | `RESP_CODE_EXPORT_CONTACT` |
| 12 | `0x0C` | `RESP_CODE_BATT_AND_STORAGE` |
| 13 | `0x0D` | `RESP_CODE_DEVICE_INFO` |
| 14 | `0x0E` | `RESP_CODE_PRIVATE_KEY` |
| 15 | `0x0F` | `RESP_CODE_DISABLED` |
| 16 | `0x10` | `RESP_CODE_CONTACT_MSG_RECV_V3` (ab App-Version 3) |
| 17 | `0x11` | `RESP_CODE_CHANNEL_MSG_RECV_V3` (ab App-Version 3) |
| 18 | `0x12` | `RESP_CODE_CHANNEL_INFO` |
| 19 | `0x13` | `RESP_CODE_SIGN_START` |
| 20 | `0x14` | `RESP_CODE_SIGNATURE` |
| 21 | `0x15` | `RESP_CODE_CUSTOM_VARS` |
| 22 | `0x16` | `RESP_CODE_ADVERT_PATH` |
| 23 | `0x17` | `RESP_CODE_TUNING_PARAMS` |
| 24 | `0x18` | `RESP_CODE_STATS` (v8+, zweites Byte ist der Statistiktyp) |
| 25 | `0x19` | `RESP_CODE_AUTOADD_CONFIG` |
| 26 | `0x1A` | `RESP_ALLOWED_REPEAT_FREQ` (ohne `_CODE_`, so im Quelltext) |
| 27 | `0x1B` | `RESP_CODE_CHANNEL_DATA_RECV` |
| 28 | `0x1C` | `RESP_CODE_DEFAULT_FLOOD_SCOPE` |

Die V3-Varianten liefern zusätzlich SNR. **Welche Variante kommt, entscheidet
die App**, nicht die Firmware — siehe „Protokollversionen" oben.

### Pushes (Radio → App, unaufgefordert) — Werte `SOURCE`

| Hex | Name |
| --- | --- |
| `0x80` | `PUSH_CODE_ADVERT` |
| `0x81` | `PUSH_CODE_PATH_UPDATED` |
| `0x82` | `PUSH_CODE_SEND_CONFIRMED` |
| `0x83` | `PUSH_CODE_MSG_WAITING` |
| `0x84` | `PUSH_CODE_RAW_DATA` |
| `0x85` | `PUSH_CODE_LOGIN_SUCCESS` |
| `0x86` | `PUSH_CODE_LOGIN_FAIL` |
| `0x87` | `PUSH_CODE_STATUS_RESPONSE` |
| `0x88` | `PUSH_CODE_LOG_RX_DATA` |
| `0x89` | `PUSH_CODE_TRACE_DATA` |
| `0x8A` | `PUSH_CODE_NEW_ADVERT` |
| `0x8B` | `PUSH_CODE_TELEMETRY_RESPONSE` |
| `0x8C` | `PUSH_CODE_BINARY_RESPONSE` |
| `0x8D` | `PUSH_CODE_PATH_DISCOVERY_RESPONSE` |
| `0x8E` | `PUSH_CODE_CONTROL_DATA` (v8+) |
| `0x8F` | `PUSH_CODE_CONTACT_DELETED` (Kontakt beim Überschreiben verdrängt) |
| `0x90` | `PUSH_CODE_CONTACTS_FULL` (Kontaktspeicher voll) |

`PUSH_CODE_MSG_WAITING` ist der Kern des Empfangsablaufs: Der Node meldet nur,
dass etwas anliegt; der Client holt die Nachrichten einzeln mit
`CMD_SYNC_NEXT_MESSAGE` ab, bis `RESP_CODE_NO_MORE_MESSAGES` kommt.

### Fehlercodes (Payload von `RESP_CODE_ERR`) — Werte `SOURCE`

| Wert | Name | Bedeutung |
| --- | --- | --- |
| 1 | `ERR_CODE_UNSUPPORTED_CMD` | Kommando nicht implementiert |
| 2 | `ERR_CODE_NOT_FOUND` | Ziel nicht gefunden |
| 3 | `ERR_CODE_TABLE_FULL` | Tabelle/Warteschlange voll, später erneut |
| 4 | `ERR_CODE_BAD_STATE` | Gerät nicht im passenden Zustand |
| 5 | `ERR_CODE_FILE_IO_ERROR` | Dateisystemfehler |
| 6 | `ERR_CODE_ILLEGAL_ARG` | Argument ungültig |

## Was MeshDash davon braucht

Die früher vermissten Fähigkeiten haben jetzt alle einen belegten Opcode. Nach
Modul der [Roadmap](../roadmap.md) sortiert:

| Modul | Kommando | Antwort bzw. Push |
| --- | --- | --- |
| `system` | `CMD_DEVICE_QUERY` 22, `CMD_APP_START` 1, `CMD_GET_BATT_AND_STORAGE` 20 | `RESP_CODE_DEVICE_INFO` 13, `RESP_CODE_SELF_INFO` 5, `RESP_CODE_BATT_AND_STORAGE` 12 |
| `nodes` | `CMD_GET_CONTACTS` 4, `CMD_GET_CONTACT_BY_KEY` 30, `CMD_GET_ADVERT_PATH` 42, `CMD_SEND_PATH_DISCOVERY_REQ` 52 | `RESP_CODE_CONTACTS_START` 2 / `RESP_CODE_CONTACT` 3 / `RESP_CODE_END_OF_CONTACTS` 4, `RESP_CODE_ADVERT_PATH` 22, `PUSH_CODE_ADVERT` `0x80`, `PUSH_CODE_NEW_ADVERT` `0x8A`, `PUSH_CODE_PATH_UPDATED` `0x81`, `PUSH_CODE_PATH_DISCOVERY_RESPONSE` `0x8D` |
| `messages` | `CMD_SEND_TXT_MSG` 2, `CMD_SEND_CHANNEL_TXT_MSG` 3, `CMD_SYNC_NEXT_MESSAGE` 10 | `PUSH_CODE_MSG_WAITING` `0x83`, `RESP_CODE_CONTACT_MSG_RECV_V3` 16, `RESP_CODE_CHANNEL_MSG_RECV_V3` 17, `RESP_CODE_SENT` 6, `PUSH_CODE_SEND_CONFIRMED` `0x82` |
| `telemetry` | `CMD_SEND_TELEMETRY_REQ` 39, `CMD_GET_STATS` 56 | `PUSH_CODE_TELEMETRY_RESPONSE` `0x8B`, `RESP_CODE_STATS` 24 |
| `admin` (später) | `CMD_SEND_LOGIN` 26, `CMD_SEND_STATUS_REQ` 27, `CMD_LOGOUT` 29 | `PUSH_CODE_LOGIN_SUCCESS` `0x85`, `PUSH_CODE_LOGIN_FAIL` `0x86`, `PUSH_CODE_STATUS_RESPONSE` `0x87` |

Zwei Pushes verdienen besondere Beachtung, weil sie stillen Datenverlust
anzeigen: `PUSH_CODE_CONTACT_DELETED` (`0x8F`) meldet, dass ein Kontakt beim
Überschreiben verdrängt wurde, `PUSH_CODE_CONTACTS_FULL` (`0x90`), dass der
Speicher voll ist. Ein Dashboard, das den Verlauf führt, sollte beides
mitschreiben statt es zu verwerfen.

## Belegte Payload-Details

Bruchstückhaft, aber `SOURCE`. Alles Übrige unten unter „Offene Fragen".

**SNR ist mit 4 multipliziert.** Die Firmware schreibt
`(int8_t)(pkt->getSNR() * 4)` in die V3-Varianten. Der Wert ist vorzeichenbehaftet
und muss zum Anzeigen durch 4 geteilt werden. Wer das übersieht, bekommt
plausibel aussehende, aber vierfach zu große Werte — genau die Sorte Fehler, die
niemandem auffällt.

**`RESP_CODE_DEVICE_INFO`** wird in `handleCmdFrame()` in dieser Reihenfolge
zusammengesetzt: Opcode, `FIRMWARE_VER_CODE` (1 B), `MAX_CONTACTS / 2` (1 B, v3+),
`MAX_GROUP_CHANNELS` (1 B, v3+), BLE-PIN (4 B), Build-Datum (12 B, nullterminiert),
Herstellername (40 B), Firmware-Version (20 B), Repeater-Flag (1 B, v9+).

## Offene Fragen

Alle verbleibenden Fragen betreffen **Payload-Aufteilungen**, nicht mehr die
Opcodes. Sie lassen sich einzeln in `handleCmdFrame()` und den `on…Recv()`-
Methoden von `MyMesh.cpp` klären — dieselbe Datei, nur weiter unten.

1. Feldaufteilung von `RESP_CODE_SELF_INFO` (Identität und Funkkonfiguration).
2. Aufbau der Kontaktstruktur in `RESP_CODE_CONTACT`.
3. Format der Advert-Paketdaten in `PUSH_CODE_ADVERT` und `PUSH_CODE_NEW_ADVERT`.
4. Aufbau von `RESP_CODE_STATS` je Statistiktyp und von
   `PUSH_CODE_TELEMETRY_RESPONSE` (CayenneLPP — die Firmware nutzt dafür eine
   eigene Bibliothek, das Format ist also nicht projektspezifisch).
5. Genaue Kodierung der Pfadangaben (`path`, `path_len`) in den Nachrichten- und
   Pfad-Antworten.
6. Welche Protokollversion MeshDash in `CMD_DEVICE_QUERY` ansagen soll. Das ist
   keine reine Recherchefrage, sondern eine Entscheidung: Version 3 bringt SNR,
   Version 8 und 9 bringen weitere Felder — aber jede Stufe setzt voraus, dass
   wir die zugehörigen Antwortformate auch auswerten können.

**Erledigt am 2026-08-16:**

- Framing (Belege im Abschnitt „Framing"): Richtung der Serial-Marker, Zählweise
  des Längenfelds, Prüfsumme, Framing bei TCP.
- Sämtliche Opcode-Werte für Kommandos, Antworten, Pushes und Fehlercodes —
  vollständig statt bruchstückhaft, auf Stufe `SOURCE`.
- Die Frage „ab welcher Firmware-Version kommen die V3-Varianten" war **falsch
  gestellt**: Es hängt nicht an der Firmware, sondern an der Version, die die App
  selbst ansagt.

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

### Für die Opcodes — Stufe `SOURCE`

Firmware `meshcore-dev/MeshCore`, Commit `d929643`, Firmware-Version `v1.17.1`,
`FIRMWARE_VER_CODE` 13, Build-Datum 14. August 2026:

- [`examples/companion_radio/MyMesh.cpp`](https://github.com/meshcore-dev/MeshCore/blob/d92964352441e53b93e8667b802e04f6e072b39e/examples/companion_radio/MyMesh.cpp)
  — sämtliche Opcode-`#define`s am Dateikopf; `handleCmdFrame()` wertet die
  Kommandos aus; die Versionsverzweigungen auf `app_target_ver` zeigen, welche
  Antwortvarianten wann gesendet werden
- [`examples/companion_radio/MyMesh.h`](https://github.com/meshcore-dev/MeshCore/blob/d92964352441e53b93e8667b802e04f6e072b39e/examples/companion_radio/MyMesh.h)
  — Firmware-Version und -Datum, Anfragetypen

### Für die Payload-Aufteilungen — Stufe `DOKU`

Die Tabellen dieser Quellen sind nachweislich unvollständig und in Namen
ungenau; sie taugen als Hinweis, nicht als Beleg.

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

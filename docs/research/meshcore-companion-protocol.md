# MeshCore Companion-Protokoll — Recherchestand

**Stand: 2026-08-20.** Verifikationsstufen nach [`README.md`](README.md).

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

#### Baudrate (nur USB/Serial)

**115200** — `Serial.begin(115200)` in `examples/companion_radio/main.cpp`,
MeshCore-Commit `d929643`. Gilt für die USB-Konsole des Companion-Node; TCP
kennt naturgemäß keine Baudrate.

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

**`RESP_CODE_BATT_AND_STORAGE`** (11 Byte, `handleCmdFrame()`): Opcode,
Batteriespannung in **Millivolt** (u16), belegter Speicher in KiB (u32),
Gesamtspeicher in KiB (u32). Betrifft den **angeschlossenen** Node, nicht das
Mesh. Die Firmware meldet Spannung, keinen Ladestand — der ließe sich ohne
Zellchemie und -zahl nicht ableiten. Umgesetzt in `meshdash_proto::battery`.

**Telemetrie fremder Nodes steckt in CayenneLPP.**
`PUSH_CODE_TELEMETRY_RESPONSE` trägt Opcode, ein reserviertes Byte, ein
6-Byte-Schlüsselpräfix und danach die Nutzdaten des Antwortpakets ab Offset 4 —
und die sind CayenneLPP, ein Fremdformat. Der Rahmen ist damit belegt, der
Inhalt nicht.

**Der Absender einer Nachricht wird nur mit sechs Byte benannt.**
`RESP_CODE_CONTACT_MSG_RECV(_V3)` überträgt nicht den vollen Schlüssel, sondern
ein 6-Byte-Präfix (`just 6-byte prefix`, `queueMessage()`). Eine Zuordnung zu
einem Kontakt ist damit ein Präfixvergleich — und Präfixe können kollidieren.
Wer darauf aufbaut, behandelt den Treffer als „vermutlich dieser Kontakt", nicht
als sicher.

**Eine Pfadlänge von `0xFF` heißt „kein Flood-Pfad"**, nicht 255 Zwischenschritte.
Die Firmware schreibt den Wert, sobald das Paket nicht als Flood lief.

**Der Nachrichtentext läuft bis zum Frame-Ende und ist nicht terminiert.** Die
Firmware kürzt ihn auf die Rahmengröße **ohne Rücksicht auf Zeichengrenzen** —
der Quelltext vermerkt selbst `TODO: UTF-8 ??`. Das letzte Zeichen kann also
halbiert ankommen; ein Parser muss das aushalten, statt die Nachricht zu
verwerfen.

**Vier Zusatzbytes gibt es nur bei signierten Nachrichten.** Zwischen Zeitstempel
und Text steht bei `TXT_TYPE_SIGNED_PLAIN` (2) ein 4-Byte-Signaturpräfix, bei
`TXT_TYPE_PLAIN` (0) und `TXT_TYPE_CLI_DATA` (1) nicht. Eine feste Annahme
verschluckt entweder vier Zeichen oder stellt dem Text vier Bytes Unsinn voran.
Konstanten aus `src/helpers/TxtDataHelpers.h`.

**Der Kontaktabruf ist ein Strom, keine einzelne Antwort.** Auf
`CMD_GET_CONTACTS` (optional mit `since` als u32 ab Byte 1) folgt
`RESP_CODE_CONTACTS_START` mit der Anzahl, danach je Kontakt ein
`RESP_CODE_CONTACT`, zuletzt `RESP_CODE_END_OF_CONTACTS` mit dem jüngsten
`lastmod`. Wer nur den ersten Frame als Antwort nimmt, verliert die Liste und
bringt den Austausch dauerhaft aus dem Tritt.

**Die Zahl in `CONTACTS_START` ist die Gesamtzahl, nicht die gefilterte.** Der
Quelltext sagt es ausdrücklich: `total, NOT filtered count`. Wer darauf wartet,
dass so viele Kontakte eintreffen, wartet bei gesetztem `since` vergeblich —
das Ende erkennt man nur am Abschluss-Frame.

**Das `lastmod` im Abschluss-Frame ist der Wert für das nächste `since`.** Damit
holt ein Client beim nächsten Mal nur Geändertes.

**Koordinaten sind Mikrograd, keine Grad.** Die Firmware multipliziert beim
Setzen mit `1e6`, dividiert beim Lesen und weist Werte jenseits von ±90e6 bzw.
±180e6 zurück (`CMD_SET_ADVERT_LATLON`, `MyMesh.cpp`). Wer den rohen Wert als
Grad nimmt, landet weit hinter den Polen.

**Ein Pfadfeld ist breiter als der Pfad.** `RESP_CODE_CONTACT` überträgt
`out_path` immer mit `MAX_PATH_SIZE` = 64 Byte; gültig sind nur die ersten
`out_path_len`. Wer alles liest, erfindet Zwischenstationen aus dem, was ein
früherer, längerer Pfad hinterlassen hat — eine Route, die plausibel aussieht
und nie existierte.

**`PUB_KEY_SIZE` ist 32, `MAX_PATH_SIZE` ist 64** (`src/MeshCore.h`,
Commit `d929643`).

**`RESP_CODE_CONTACT`** (`writeContactRespFrame()`, 148 Byte): Opcode,
Pubkey (32 B), Typ (1 B), Flags (1 B), `out_path_len` (1 B), Pfad (64 B),
Name (32 B, nullterminiert), letzter Advert (u32), Breite (i32), Länge (i32),
letzte Änderung (u32). Umgesetzt in `meshdash_proto::contact`.

**Ein zu langer Rahmen wird abgeschnitten, nicht verworfen** — Stufe `SOURCE`,
`ArduinoSerialInterface::checkRecvFrame()`, Commit `d929643`. Der
Empfangspuffer ist `uint8_t rx_buf[MAX_FRAME_SIZE]`, also genau 176 Byte
Nutzlast ohne den 3-Byte-Rahmen. Kommt mehr, liest die Firmware weiter, wirft
den Überhang weg und kürzt die Länge:

```c
if (_frame_len > MAX_FRAME_SIZE) _frame_len = MAX_FRAME_SIZE;    // truncate
```

Der Node verarbeitet dann die ersten 176 Byte, **als wären sie der ganze
Rahmen**. Bei einer Textnachricht heißt das: still gekürzter Text. Bei einem
Rahmen mit Feldern hinter dem Text hieße es: Unsinn in den hinteren Feldern.
Wer zu lang sendet, bekommt also keinen Fehler zurück — er bekommt etwas
Falsches. Die Sendegrenze ist deshalb Pflicht des Absenders, nicht des Node.

**Die beiden Advert-Pushes tragen sehr unterschiedlich viel** — Stufe `SOURCE`,
`onDiscoveredContact()`, Commit `d929643`. Welcher der beiden kommt, entscheidet
allein, ob der Node den Kontakt schon kannte:

- `PUSH_CODE_NEW_ADVERT` (`0x8A`) — kannte er nicht. Die Firmware ruft dafür
  **dieselbe** `writeContactRespFrame()` auf wie für `RESP_CODE_CONTACT`; die
  Nutzlast ist also Byte für Byte ein Kontakt, nur mit anderem ersten Byte.
- `PUSH_CODE_ADVERT` (`0x80`) — kannte er. Es reisen Opcode und Pubkey (32 B),
  sonst nichts. Insgesamt 33 Byte.

Die kurze Form ist **kein abgespeckter Kontakt**, sondern die Aussage „dieser
Schlüssel wurde eben gehört". Name, Typ, Pfad und Position fehlen, weil der Node
davon ausgeht, dass die App sie aus der Kontaktliste hat — nicht, weil sie leer
geworden wären. Wer auf diesen Push hin die fehlenden Felder schreibt, löscht,
was die Liste geliefert hat. Umgesetzt in `meshdash_proto::advert`.

Nebenbei belegt dieselbe Funktion: Der Node führt zusätzlich eine eigene Tabelle
zuletzt gehörter Adverts samt Rückpfad (`advert_paths`, `getRecentlyHeard()`).
Ob und wie sie über das Companion-Protokoll abrufbar ist, ist noch nicht geprüft.

**Senden — `CMD_SEND_TXT_MSG` (2)**, Stufe `SOURCE`, `handleCmdFrame()`,
Commit `d929643`. Die Firmware nimmt den Zweig nur bei **mindestens 14 Byte**:

```text
0   1  Opcode
1   1  txt_type (TXT_TYPE_PLAIN oder TXT_TYPE_CLI_DATA, sonst Fehler)
2   1  attempt — Zähler für Wiederholungen desselben Textes
3   4  Zeitstempel (u32 LE), kommt von der App
7   6  Pubkey-Präfix des Empfängers — sechs Byte, wie überall
13  n  Text bis Frame-Ende, ohne Nullterminierung
```

Ein **leerer Text** unterschreitet die 14 Byte. Der Zweig greift dann nicht, und
das Kommando läuft in das Ende der Kette — der Node antwortet also mit einem
Fehler über ein unbekanntes Kommando, nicht mit „leere Nachricht". Wer das nicht
vorher abfängt, meldet dem Betreiber den falschen Grund.

Bei `TXT_TYPE_CLI_DATA` **überschreibt die Firmware den Zeitstempel** mit ihrer
eigenen Uhr, um den Replay-Schutz der Gegenstelle nicht auszulösen. Der
mitgeschickte Wert wird in diesem Fall verworfen.

**`RESP_CODE_SENT` (6)**, 10 Byte — die Antwort darauf:

```text
0  1  Opcode
1  1  1 = als Flood gesendet, 0 = über bekannten Pfad
2  4  erwartete Quittung (u32 LE); 0 heißt „keine erwartet"
6  4  geschätzte Wartezeit in Millisekunden (u32 LE)
```

Schlägt das Einreihen fehl, kommt stattdessen `RESP_CODE_ERR` mit
`ERR_CODE_TABLE_FULL`; ist der Empfänger unbekannt, `ERR_CODE_NOT_FOUND`.

**`PUSH_CODE_SEND_CONFIRMED` (`0x82`)**, 9 Byte, aus `processAck()`: Opcode,
Quittung (4 B, passend zu `RESP_CODE_SENT`), Laufzeit in Millisekunden (4 B).
Die Firmware warnt im eigenen Quelltext: **dieselbe Quittung kann mehrfach
eintreffen.** Wer daraus zählt, zählt zu hoch.

**Kanäle senden — `CMD_SEND_CHANNEL_TXT_MSG` (3)**:

```text
0  1  Opcode
1  1  txt_type — muss TXT_TYPE_PLAIN sein, sonst ERR_CODE_UNSUPPORTED_CMD
2  1  Kanalindex
3  4  Zeitstempel (u32 LE)
7  n  Text bis Frame-Ende
```

**Die Antwort ist hier `RESP_CODE_OK`, nicht `RESP_CODE_SENT`.** Ein Broadcast
wird von niemandem quittiert, es gibt also keine Zustellung, auf die man warten
könnte. Wer auf eine Quittung wartet, wartet für immer.

**Kanalnachrichten empfangen — `RESP_CODE_CHANNEL_MSG_RECV_V3` (17) bzw.
`RESP_CODE_CHANNEL_MSG_RECV` (8)**, aus `onChannelMessageRecv()`:

```text
Offset  Größe  Feld                            nur V3
     0      1  Opcode
     1      1  SNR, mit vier multipliziert        ja
     2      2  reserviert                         ja
     +      1  Kanalindex
     +      1  Pfadlänge, 0xFF wenn kein Flood
     +      1  txt_type
     +      4  Zeitstempel (u32 LE)
     +      …  Text bis Frame-Ende
```

**Es gibt kein Absenderfeld.** Die sendende Firmware schreibt den Node-Namen in
den Text hinein, bevor sie sendet. Wer den Absender auswerten will, hat nur
Fließtext — nichts, was Code prüfen könnte.

Entscheidend für den Ablauf: Diese Frames landen wie Direktnachrichten in der
**Offline-Queue** und werden mit `PUSH_CODE_MSG_WAITING` angekündigt. Sie kommen
also über `CMD_SYNC_NEXT_MESSAGE` herein. Ein Abrufer, der nur Direktnachrichten
kennt, bleibt an der ersten Kanalnachricht stehen.

Dasselbe gilt für `RESP_CODE_CHANNEL_DATA_RECV` (27) aus `onChannelDataRecv()`:
Opcode, SNR×4, zwei reservierte Byte, Kanalindex, Pfadlänge, `data_type` (u16
LE), `data_len` (u8), Daten. Auch dieses Frame geht durch dieselbe Warteschlange.

**`RESP_CODE_CHANNEL_INFO` (18)**, 50 Byte, Antwort auf `CMD_GET_CHANNEL` (31):

```text
0   1  Opcode
1   1  Kanalindex
2  32  Name, nullterminiert
34 16  gemeinsamer Schlüssel (128 Bit)
```

**Der Schlüssel ist ein Geheimnis.** Wer ihn hat, kann den Kanal mitlesen und in
ihm senden. MeshDash liest ihn deshalb gar nicht erst aus dem Frame — was nicht
existiert, kann nicht ins Log, in eine API-Antwort oder in ein Backup geraten.

Es gibt **kein Kommando, das die Kanäle auflistet** — nur „beschreibe Index N".
Ab dem ersten unbekannten Index antwortet der Node `ERR_CODE_NOT_FOUND`; dort
endet die Liste.

**Die Kontaktkapazität steht halbiert auf der Leitung.** In
`RESP_CODE_DEVICE_INFO` schreibt die Firmware `MAX_CONTACTS / 2` in ein einzelnes
Byte, weil der echte Wert dort nicht hineinpasst. Wer das Byte als Kapazität
liest, meldet einem Betreiber 50 Kontakte, wo 100 hineinpassen — falsch, ohne
dass es auffällt. Verdoppeln ist Pflicht.

**Die Markierungen `v3+`, `v9+` und `v10+` betreffen die App, nicht den Node.**
Sie sagen, ab welcher angesagten Protokollversion eine App das Feld versteht —
der Node schreibt es unabhängig davon immer. Ältere Firmware kann trotzdem
kürzere Frames senden, ein Parser muss also mit fehlenden Endfeldern umgehen.

**`RESP_CODE_DEVICE_INFO`** wird in `handleCmdFrame()` in dieser Reihenfolge
zusammengesetzt: Opcode, `FIRMWARE_VER_CODE` (1 B), `MAX_CONTACTS / 2` (1 B, v3+),
`MAX_GROUP_CHANNELS` (1 B, v3+), BLE-PIN (4 B), Build-Datum (12 B, nullterminiert),
Herstellername (40 B), Firmware-Version (20 B), Repeater-Flag (1 B, v9+),
`path_hash_mode` (1 B, v10+). Zeichenketten sind mit Nullbytes aufgefüllt.
Vollständig umgesetzt in `meshdash_proto::device`.

## Offene Fragen

Alle verbleibenden Fragen betreffen **Payload-Aufteilungen**, nicht mehr die
Opcodes. Sie lassen sich einzeln in `handleCmdFrame()` und den `on…Recv()`-
Methoden von `MyMesh.cpp` klären — dieselbe Datei, nur weiter unten.

1. Feldaufteilung von `RESP_CODE_SELF_INFO` (Identität und Funkkonfiguration).
2. Bedeutung der Werte in `type` und `flags` eines Kontakts — die Struktur ist
   belegt, die Kodierung dieser beiden Bytes nicht.
3. Aufbau von `RESP_CODE_STATS` je Statistiktyp und von
   `PUSH_CODE_TELEMETRY_RESPONSE` (CayenneLPP — die Firmware nutzt dafür eine
   eigene Bibliothek, das Format ist also nicht projektspezifisch).
4. Genaue Kodierung der Pfadangaben (`path`, `path_len`) in den Pfad-Antworten.
   Für Nachrichten ist sie geklärt: ein Byte, `0xFF` heißt „kein Flood-Pfad".
5. Ab wann MeshDash eine **höhere** Protokollversion als 3 ansagen sollte.
   Version 3 ist gesetzt (`meshdash_proto::device::PROTOCOL_VERSION`), weil sie
   die SNR-Varianten der Nachrichten bringt. Version 8 schaltet Statistiken
   frei — dafür müssen deren Formate aber erst verifiziert sein.

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

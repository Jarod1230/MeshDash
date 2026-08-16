# ADR-0003: Serial und TCP zuerst, BLE später

- **Status:** Angenommen
- **Datum:** 2026-08-16
- **Betrifft:** `meshdash-transport`

## Kontext

Ein MeshCore-Companion-Node ist über drei Wege erreichbar: USB/Serial, TCP über
WiFi und BLE. Alle drei zu implementieren kostet Zeit, die am Anfang woanders
besser aufgehoben ist.

Hinzu kommt: **Das Framing unterscheidet sich je Transport.** BLE braucht keins,
weil die Characteristic die Frame-Grenzen liefert; Serial und TCP brauchen ein
Längenpräfix, weil ein Bytestrom keine Grenzen kennt. Siehe
[`../research/meshcore-companion-protocol.md`](../research/meshcore-companion-protocol.md).
Die Transporte sind also nicht bloß austauschbare Rohre.

## Entscheidung

**Serial und TCP** werden zuerst implementiert. **BLE** wird als Transport
vorgesehen, aber erst später gebaut. Zusätzlich entsteht von Anfang an ein
**Mock-Transport**.

Das `Transport`-Trait wird so entworfen, dass BLE später ohne Änderung an
Protokoll- oder Fachschicht ergänzt werden kann.

## Begründung

- **MeshDash läuft dauerhaft, nicht am Laptop.** Der typische Aufbau ist ein
  Companion-Node am USB-Port eines Kleinrechners, der 24/7 läuft. Das ist
  Serial. TCP deckt denselben Fall für Nodes mit WiFi ab.
- **BLE passt nicht zum Einsatzzweck.** BLE ist für mobile Clients gedacht, die
  sich verbinden und wieder trennen. Für einen Dauerbetrieb ist es die
  fragilste Option — Verbindungsabbrüche, Reichweite, unterschiedliche
  Bluetooth-Stacks je Betriebssystem.
- **Serial und TCP teilen sich das Framing.** Beide sind Bytestrom mit
  Längenpräfix. Der zweite Transport kostet danach fast nichts mehr.
- **Der Mock-Transport ist keine Zugabe.** Ohne ihn braucht jeder Test und jede
  Frontend-Entwicklung ein echtes Funkgerät. Er gehört deshalb in denselben
  Schritt wie die echten Transporte, nicht später.

## Verworfene Alternativen

**Alle drei Transporte sofort.** Vollständigkeit von Anfang an. Verworfen: BLE
ist der aufwendigste und am wenigsten gebrauchte, und der Aufwand ginge vom
Protokoll ab — der Stelle, an der das Projekt tatsächlich steht oder fällt.

**Nur Serial.** Noch kleinerer Anfang. Verworfen, weil TCP nach dem
Serial-Framing fast geschenkt ist und WiFi-Nodes real verbreitet sind.

**BLE zuerst, weil die Protokolldokumentation BLE am besten beschreibt.**
Ein ernstzunehmendes Argument — die verfügbare Doku behandelt fast nur BLE, das
Serial-Framing ist widersprüchlich dokumentiert. Trotzdem verworfen: Man würde
den Transport bauen, den man am wenigsten braucht, und stünde beim
Dauerbetriebsfall vor demselben ungeklärten Framing. Die richtige Antwort ist,
das Serial-Framing zu **verifizieren**, nicht ihm auszuweichen.

**Kein Mock-Transport, stattdessen Tests gegen echte Hardware.** Verworfen:
funktioniert in keiner CI und schließt alle Mitwirkenden ohne Node aus.

## Konsequenzen

**Positiv:** schnellster Weg zu einem nutzbaren Dashboard im tatsächlichen
Einsatzszenario; Entwicklung ohne Hardware möglich; die Transport-Abstraktion
wird an zwei echten Implementierungen plus Mock erprobt, statt an einer erraten
zu werden.

**Negativ:** Wer seinen Node nur per BLE erreicht, kann MeshDash zunächst nicht
nutzen. Das ist bewusst in Kauf genommen.

**Zu beachten:** Das `Transport`-Trait darf **kein Längenpräfix voraussetzen**,
sonst passt BLE später nicht hinein. Die Frame-Abgrenzung gehört in die
jeweilige Transport-Implementierung, nicht in die gemeinsame Schnittstelle. Das
ist die eine Designentscheidung, die dieser ADR erzwingt.

## Wann diese Entscheidung neu zu prüfen ist

- Wenn BLE konkret nachgefragt wird — dann bauen, die Abstraktion steht.
- Wenn sich herausstellt, dass TCP ein anderes Framing verwendet als Serial
  (offene Frage 4 der Protokollrecherche). Dann ist die Annahme „TCP ist fast
  geschenkt" falsch und die Reihenfolge neu zu bewerten.

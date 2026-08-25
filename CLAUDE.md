# CLAUDE.md

Arbeitsanweisung für KI-Agenten in diesem Repository. Gilt für Claude Code und
sinngemäß für jeden anderen Agenten (siehe `AGENTS.md`).

## Was MeshDash ist

Dashboard und Administrationsoberfläche für ein MeshCore-LoRa-Mesh. Ein Rust-Backend
spricht über Serial oder TCP mit einem MeshCore-Companion-Node, persistiert dessen
Ereignisse und stellt sie einem React-Frontend als REST-API und WebSocket-Stream
bereit. Ausgeliefert wird ein einzelnes Binary mit eingebettetem Frontend.

**Projektstand: Dienst und Oberfläche laufen (Schritte 1 bis 7 erledigt).**
Protokoll-Codec, Transport mit Reconnect, Kern mit Datenbank und Event-Bus,
HTTP-Server mit Authentifizierung und WebSocket sowie vier Module — `system`,
`nodes`, `messages`, `telemetry`. Der Dienst läuft und liefert Daten unter
`/api/v1/`.

**Wohin es geht: die Karte wird die Leitansicht.** Eine Region mit den Knoten
darin, auf der Empfang und Verkehr sichtbar werden; die Listen bleiben daneben.
Entschieden in `docs/decisions/0011-karte-als-leitansicht.md`, der Weg dorthin
steht als Stufen A bis D in `docs/roadmap.md`. Wer an der Oberfläche baut,
liest das Zielbild zuerst — sonst entsteht eine weitere Seite neben der Karte
statt einer Ebene auf ihr.

**Zum Protokoll:** Framing, sämtliche Opcodes und die bisher benötigten
Nutzlasten sind am Firmware-Quellcode verifiziert (Commit `d929643`). Nicht
verifiziert sind der Aufbau des rohen Pakets aus `PUSH_CODE_RX_LOG_DATA`, die
Bildung der Pfad-Hashes, der Rahmen der Pfad-Antworten sowie die Bedeutung der
Bytes `type` und `flags` eines Kontakts. Die ersten drei stehen als Stufe A der
Roadmap an, weil die Karte darauf aufbaut. Für alles Unverifizierte gilt Regel 1 unten
unverändert — auch dann, wenn danebenliegende Werte längst belegt sind.

## Wo was steht

| Frage | Datei |
| --- | --- |
| Wie ist das System geschnitten? | `docs/architecture.md` |
| Wie baue ich ein Feature? | `docs/module-system.md` |
| Wie heißen Branches, Commits, Typen? | `docs/conventions.md` |
| Wie richte ich die Umgebung ein? | `docs/development.md` |
| Warum wurde X so entschieden? | `docs/decisions/` |
| Was ist schon schiefgegangen? | `docs/lessons-learned.md` |
| Was bedeutet dieser MeshCore-Begriff? | `docs/glossary.md` |
| Was weiß ich über das Protokoll? | `docs/research/meshcore-companion-protocol.md` |
| Was ist als Nächstes dran? | `docs/roadmap.md` |

Lies bei Architektur- oder Protokollarbeit **immer zuerst** die passende Datei
oben. Sie ist der Stand des Projekts; dein Vorwissen über MeshCore ist es nicht.

## Harte Regeln

### 1. Protokollwerte werden nicht geraten

Das ist die wichtigste Regel hier. Ein falsch geratener Opcode, ein falscher
Offset oder eine falsch angenommene Feldbreite wirft **keinen Fehler** — er
produziert stillschweigend falsche Daten, die dann in der Datenbank landen.

- Jeder Opcode, jedes Offset, jede Feldbreite braucht eine belegbare Quelle:
  Upstream-Doku, Firmware-Quellcode oder eine Referenzimplementierung.
- Die Quelle kommt als Kommentar direkt an den Wert.
- Nicht Verifiziertes wird **als unverifiziert markiert** und nicht als Wahrheit
  behandelt. Lieber ein `Unknown(u8)`-Fallback als eine erfundene Konstante.
- Neue Erkenntnisse gehören mit Quelle und Datum nach
  `docs/research/meshcore-companion-protocol.md`.

Wenn dir eine Information fehlt: recherchiere sie oder markiere die Lücke.
Beides ist in Ordnung. Sie zu erfinden ist es nicht.

### 2. Kein Scope-Zuwachs ohne Auftrag

Bau, was gefragt war. Wenn dir dabei etwas anderes auffällt, notiere es in
`docs/roadmap.md` oder als Issue — aber setz es nicht nebenbei mit um.

### 3. Features sind Module

Alles, was fachlich eigenständig ist, wird als Modul gebaut, nicht in den Kern
gelegt. Wenn du dabei bist, etwas in `meshdash-core` zu schreiben, das eine
Fachlichkeit kennt, ist das der falsche Ort. `docs/module-system.md` erklärt
den Zuschnitt.

### 4. Sprachtrennung einhalten

Dokumentation, Issues und PR-Beschreibungen auf **Deutsch**. Code, Bezeichner,
Code-Kommentare, Commit-Messages und Log-Ausgaben auf **Englisch**.
Begründung in `docs/decisions/0004-dokumentationssprache.md`.

### 5. Nichts als fertig melden, was nicht läuft

Wenn Tests fehlschlagen, sag das mit Ausgabe. Wenn ein Teil offen blieb, sag,
welcher und warum. Kein „sollte jetzt funktionieren" ohne Ausführung.

## Pflegepflichten

Nach einer Änderung mitziehen — das ist Teil der Aufgabe, nicht optional:

- **Architekturentscheidung getroffen oder revidiert?** → neuer ADR in
  `docs/decisions/`. Bestehende ADRs werden nicht umgeschrieben, sondern durch
  einen neuen ADR abgelöst (Status `Abgelöst durch ADR-XXXX`).
- **Etwas gelernt, das eine Stunde gekostet hat?** → `docs/lessons-learned.md`.
  Nicht erst wenn es „wichtig genug" ist.
- **Neues Modul?** → Tabelle in `docs/module-system.md` ergänzen.
- **Neue Konfigurationsoption?** → `docs/configuration.md`.
- **Nutzersichtbare Änderung?** → `CHANGELOG.md` unter `[Unreleased]`.
- **Roadmap-Schritt abgeschlossen?** → Standangaben mitziehen: der Abschnitt
  „Projektstand" oben, der Statusblock in `README.md` und der Kopfkommentar
  jedes berührten Crates. Die veralten still — niemand merkt es beim Bauen.

## Arbeitsweise

- Branch von `main`, Namensschema in `docs/conventions.md`. Nie direkt auf `main`.
- **Jede Änderung geht über einen Pull Request** — ausnahmslos, auch reine
  Doku-Commits und Einzeiler. Der PR ist die Übersicht über das, was passiert
  ist; ein lokal durchgereichter Commit fehlt darin. Also: nie vorschlagen,
  einen Branch lokal zu mergen.
- Conventional Commits.
- Vor dem Melden von Fertigstellung: `just check`. Das ist genau das, was die CI
  fährt — Format, Clippy, Rust-Tests, Frontend-Lint/Typen/Tests/Build und die
  Prüfung interner Doku-Links.
- Tests für Protokoll-Parsing sind Pflicht, nicht Kür.

## Umgebung

Rust, Go, Node 22, pnpm, Python und Docker sind in der Entwicklungsumgebung
vorhanden. Ausgehende Verbindungen laufen über einen Proxy; crates.io und die
npm-Registry sind erreichbar.

Ohne angeschlossene Hardware lässt sich das Protokoll nicht end-to-end testen.
Plane deshalb von Anfang an einen Mock-Transport ein, statt Hardware
vorauszusetzen — siehe `docs/architecture.md`, Abschnitt „Testbarkeit".

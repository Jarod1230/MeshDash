# CLAUDE.md

Arbeitsanweisung für KI-Agenten in diesem Repository. Gilt für Claude Code und
sinngemäß für jeden anderen Agenten (siehe `AGENTS.md`).

## Was MeshDash ist

Dashboard und Administrationsoberfläche für ein MeshCore-LoRa-Mesh. Ein Rust-Backend
spricht über Serial oder TCP mit einem MeshCore-Companion-Node, persistiert dessen
Ereignisse und stellt sie einem React-Frontend als REST-API und WebSocket-Stream
bereit. Ausgeliefert wird ein einzelnes Binary mit eingebettetem Frontend.

**Projektstand (2026-09-06): Dienst und Oberfläche laufen.** Protokoll-Codec,
Transport mit Reconnect, Kern mit Datenbank, Event-Bus und zur Laufzeit
änderbaren Einstellungen, HTTP-Server mit Authentifizierung und WebSocket sowie
**sechs Module** — `system`, `nodes`, `messages`, `telemetry`, `tiles`,
`traffic`.

**Die Karte ist die Leitansicht.** MeshDash öffnet auf der Fläche mit den
Knoten darauf; die Seiten liegen als Blende darüber und die Fläche wird nie
neu aufgebaut. Sie zeichnet in Web-Mercator selbst, mit Kacheln über den
Dienst, Knoten- und Verbindungsebene, laufenden Paketen, Zeitraumwähler und
Abspielen für Verbindungen sowie Kontexttafeln für Knoten und Verbindungen.

Entschieden in `docs/decisions/0011-karte-als-leitansicht.md`, die Adresse in
`0014`, der Verzicht auf Leaflet in `0015`, die Zeitbedeutung in `0018`. Der
Weg steht als **Stufen A bis D** in `docs/roadmap.md`: A, B und C sind
erledigt. D ist unangetastet.

Wer an der Oberfläche baut, liest **`docs/frontend.md`** zuerst — sonst
entsteht eine weitere Seite neben der Karte statt einer Ebene auf ihr.

**Zum Protokoll:** Framing, sämtliche Opcodes und die bisher benötigten
Nutzlasten sind am Firmware-Quellcode verifiziert (Commit `d929643`) —
seit dem 2026-08-25 auch die Paketebene: Aufbau des rohen Pakets, Bildung der
Pfad-Hashes und beide Pfad-Antworten. Nicht verifiziert sind noch die Bedeutung
der Bytes `type` und `flags` eines Kontakts sowie die Fehlerflags in
`RESP_CODE_STATS`. Für alles Unverifizierte gilt Regel 1 unten
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
| Wie ist die Oberfläche geschnitten? | `docs/frontend.md` |
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

## Wenn mehrere Agenten gleichzeitig arbeiten

Seit dem 2026-09-06 arbeitet mehr als ein KI-System an diesem Repository. Das
ändert nichts an den Regeln oben, fügt aber welche hinzu. Sie stammen alle aus
Fehlern, die hier schon passiert sind.

### Was du dir nimmst

`docs/roadmap.md` ist die Warteschlange. Vorne stehen die Punkte unter
**„Aus der Benutzung gemeldet"**, danach der Rest von Stufe C, dann Stufe D.

**Bevor du anfängst: `gh pr list` und `git branch -r`.** Woran schon jemand
arbeitet, erkennst du am offenen PR oder am Zweig. Nimm dir nichts, was dort
schon läuft — auch nicht „nur den Backend-Teil davon".

**Ein Punkt, ein Zweig, ein PR.** Kein Sammel-PR über drei Punkte: Wer
gleichzeitig arbeitet, braucht kleine, schnell mergende Änderungen, sonst
kollidiert alles mit allem.

### Die vier Stellen, an denen es wirklich knallt

1. **Migrationsnummern.** Zwei Module-Migrationen mit derselben Version sind
   die teuerste Kollision hier — sie fällt erst auf, wenn eine Datenbank die
   eine schon angewandt hat und die andere still übersprungen wird. Das ist
   genau einmal passiert und steht in `lessons-learned.md`. **Vor jeder neuen
   Migration: offene PRs auf dasselbe Modul prüfen.** Migrationen werden nach
   dem Merge nie geändert.
2. **ADR-Nummern.** Zwei ADRs mit derselben Nummer. Vor dem Anlegen einmal in
   `docs/decisions/README.md` und in die offenen PRs schauen.
3. **Die Registrierungslisten.** `crates/meshdash-server/src/main.rs`,
   `web/src/modules/index.ts`, die Tabelle in `docs/module-system.md`, der
   Index in `docs/decisions/README.md`. Jeweils eine Zeile, aber alle am selben
   Fleck — hier entstehen Konflikte, und sie sind harmlos, solange man sie
   auflöst statt zu überschreiben.
4. **`CHANGELOG.md` und `docs/roadmap.md`.** Dasselbe: viele kleine
   Ergänzungen an derselben Stelle.

### Zweige und PRs

- **Immer von `main` abzweigen, immer gegen `main` mergen.** Ein PR gegen einen
  anderen Feature-Zweig ist hier schon einmal ins Leere gelaufen: Der Basiszweig
  war zum Merge-Zeitpunkt bereits in `main`, und die Commits landeten gemergt
  daneben statt drin (PR #71).
- **Nach dem Öffnen eines PR nichts mehr auf den Zweig schieben, ohne zu
  prüfen, ob er noch offen ist.** Auch das ist passiert (PR #81): gemergt,
  danach noch ein Commit, der liegenblieb.
- Vor dem Melden von Fertigstellung `git pull` auf `main` und neu prüfen, wenn
  in der Zwischenzeit etwas gemergt wurde.

### Was du dem anderen schuldest

- **Schreib auf, was du gelernt hast** — `lessons-learned.md`, nicht erst wenn
  es „wichtig genug" wirkt. Der andere Agent hat deinen Verlauf nicht.
- **Sag im PR, was du nicht geprüft hast.** Siehe `docs/testing.md`, Abschnitt
  „Gegen ein echtes Gegenüber laufen lassen".
- **Fass die laufende Arbeit des anderen nicht an.** Fällt dir in seinem Bereich
  etwas auf, notiere es in `roadmap.md` unter „Gesammelte Einfälle" statt es
  nebenbei zu reparieren. Regel 2 gilt hier doppelt.

### Hardware

Es gibt **einen** Companion-Node am USB, und nur ein Prozess kann den seriellen
Port halten. Wer den Dienst gegen echte Hardware laufen lässt, sagt es und
räumt ihn wieder ab. Ohne Hardware bleibt der Mock-Transport — siehe
`docs/testing.md`.

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

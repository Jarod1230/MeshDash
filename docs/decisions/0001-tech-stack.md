# ADR-0001: Technologie-Stack — Rust im Backend, React im Frontend

- **Status:** Angenommen
- **Datum:** 2026-08-16
- **Betrifft:** das gesamte Projekt

## Kontext

MeshDash soll eine Web-App sein, modular wachsen, schnell sein und dauerhaft
neben einem Companion-Node laufen — realistisch auf einem Raspberry Pi, nicht
auf einem Server mit reichlich RAM. Die Anwendung muss dabei:

- eine serielle Schnittstelle und TCP-Verbindungen dauerhaft und robust
  bedienen, inklusive Wiederverbindung,
- ein binäres Protokoll byteweise korrekt zerlegen,
- nebenläufig persistieren und gleichzeitig HTTP und WebSocket bedienen.

Die MeshCore-Firmware ist in C/C++ geschrieben. Das legt zunächst die Frage
nahe, ob das Backend dieselbe Sprache braucht — tut es nicht: Die Kopplung
läuft über das **Companion-Protokoll über Serial bzw. TCP**, also über Bytes
auf einer Leitung, nicht über Bibliotheks-Linking. Damit ist die Sprachwahl im
Backend frei.

## Entscheidung

Backend in **Rust**, Frontend in **React mit TypeScript und Vite**. Das gebaute
Frontend wird ins Rust-Binary eingebettet, sodass genau ein Artefakt
ausgeliefert wird.

## Begründung

Für Rust im Backend:

- **Byte-Parsing ohne stille Fehler.** Das Companion-Protokoll ist die
  fehleranfälligste Stelle des Projekts. Ein Typsystem mit erschöpfender
  Fallunterscheidung und `Result` erzwingt hier eine Behandlung aller Fälle.
- **Ressourcenverbrauch.** Auf einem Pi Zero ist der Unterschied zwischen einem
  Dauerprozess mit 15 MB und einem mit 150 MB kein Detail.
- **Ein Binary ohne Laufzeitumgebung.** Keine Runtime, kein Interpreter.
- **Serielle Schnittstellen** sind mit `tokio-serial` gut abgedeckt.

Für React im Frontend:

- Das größere Ökosystem für die Bausteine, die ein Dashboard braucht —
  Karten, Diagramme, virtualisierte Tabellen. Genau diese Komponenten wären
  sonst der Engpass.
- Große Auswahl an Mitwirkenden, die es bereits können.

## Verworfene Alternativen

**Go im Backend.** Schneller zu erlernen, schneller zu kompilieren, ebenfalls
ein einzelnes Binary. Verworfen wegen schwächerer Unterstützung für serielle
Schnittstellen und BLE, höherem Speicherbedarf durch die Garbage Collection und
— entscheidend — einem Typsystem, das beim binären Parsen weniger absichert.
Go bleibt die naheliegende Alternative, falls Rust sich als Hürde für
Mitwirkende erweist.

**C++ zur Angleichung an die Firmware.** Beruht auf einem Missverständnis: Es
gibt keine gemeinsame Codebasis, nur ein gemeinsames Wire-Format. Der Aufwand
für Speichersicherheit und Build-Ketten wäre ohne Gegenwert.

**Node.js oder Python im Backend.** Schnellster Einstieg, und für Python gibt es
mit `meshcore_py` sogar eine fertige Protokollbibliothek. Verworfen wegen
Ressourcenbedarf auf schwacher Hardware und weil eine dauerlaufende, robuste
Verbindungsverwaltung nicht die Stärke dieser Laufzeiten ist. `meshcore_py`
bleibt trotzdem wertvoll — als **Referenz** für das Protokoll, siehe
[`../research/meshcore-companion-protocol.md`](../research/meshcore-companion-protocol.md).

**SvelteKit statt React.** Kleinere Bundles, weniger Boilerplate, angenehmer zu
schreiben. Verworfen wegen des kleineren Angebots an fertigen Dashboard-Bausteinen
und der geringeren Verbreitung unter potenziellen Mitwirkenden. Die Entscheidung
war knapp.

**Getrennte Auslieferung von Frontend und Backend.** Verworfen, weil das für den
Betreiber eines Heim-Mesh zwei Dinge zu betreiben bedeutet statt einem.

## Konsequenzen

**Positiv:** ein Artefakt zum Ausliefern; geringer Ressourcenbedarf; die
kritische Protokollschicht liegt in einer Sprache, die Fehler dort früh sichtbar
macht.

**Negativ:** Rust hat eine spürbare Einstiegshürde und lange Build-Zeiten,
besonders beim Cross-Compilieren für ARM. Zwei Sprachen und zwei Paketverwaltungen
im Projekt. Das Frontend muss vor dem Backend gebaut werden — die Build-Reihenfolge
ist eine zusätzliche Fehlerquelle in der CI.

**Zu beachten:** Das Einbetten des Frontends bedeutet, dass jede Frontend-Änderung
ein neues Binary erfordert. Für den Entwicklungsmodus braucht es deshalb einen
Vite-Dev-Server mit Proxy auf das Backend — beschrieben in
[`../development.md`](../development.md).

## Wann diese Entscheidung neu zu prüfen ist

- Wenn Rust nachweislich Mitwirkende abhält, obwohl es welche gäbe.
- Wenn die Build-Zeiten für ARM den Entwicklungsfluss ernsthaft stören.

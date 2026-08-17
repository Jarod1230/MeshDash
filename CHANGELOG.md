# Changelog

Alle nennenswerten Änderungen an MeshDash werden hier festgehalten.

Format nach [Keep a Changelog](https://keepachangelog.com/de/1.1.0/),
Versionierung nach [Semantic Versioning](https://semver.org/lang/de/).

Solange die Hauptversion `0` ist, können sich APIs und Datenbankschema in
jedem Minor-Release ändern.

## [Unreleased]

### Added

- **Live-Ereignisse über WebSocket** unter `/api/v1/events`. Verbindungsstatus
  des Node und alles, was er von sich aus meldet, erreichen den Browser ohne
  Nachfragen. Ist ein Token gesetzt, wird es als erste Nachricht erwartet —
  Browser können bei WebSocket-Verbindungen keinen Header mitgeben.
- **Authentifizierung für die API.** Ist `[auth] token` gesetzt, braucht jede
  Anfrage unter `/api/v1/` ein passendes Bearer-Token. Ist es nicht gesetzt und
  lauscht MeshDash auf einer öffentlichen Adresse, **startet der Dienst nicht** —
  wer hinter einem Reverse-Proxy ohne eigenes Token betreiben will, stimmt dem
  mit `allow_unauthenticated = true` ausdrücklich zu. Grundlage ist
  ADR-0006.
- **MeshDash startet als Dienst.** Das Binary liest die Konfiguration, legt die
  Datenbank an, baut den eingestellten Transport auf, hält die Verbindung zum
  Node selbsttätig aufrecht und lauscht auf der konfigurierten Adresse.
  Modulrouten werden unter `/api/v1/<modul>/` eingehängt; solange kein Modul
  registriert ist, antwortet der Dienst auf jeden Pfad mit `404` im
  vereinbarten Fehlerformat. Authentifizierung, WebSocket und eingebettetes
  Frontend fehlen noch.
- Modulvertrag und Registry in `meshdash-core`. Ein Modul bringt Name,
  Migrationen und einen Startvorgang mit; die Registry migriert und startet
  alle. Ein fehlschlagendes Modul verhindert den Start und wird dabei benannt.
- SQLite-Anbindung in `meshdash-core`, samt Migrationen je Modul. Die
  Datenbankdatei und ihr Verzeichnis werden beim ersten Start angelegt; jedes
  Modul zählt seine Schemaversionen unabhängig von allen anderen.
- Event-Bus in `meshdash-core`. Der `Link` meldet dort, ob der Node erreichbar
  ist und was er von sich aus schickt — die Grundlage dafür, dass mehrere
  Module dieselben Ereignisse unabhängig voneinander verarbeiten können.
- Konfiguration in `meshdash-core`: `meshdash.toml` und Umgebungsvariablen mit
  Präfix `MESHDASH_`, mit Voreinstellungen für alles. MeshDash startet ohne
  Konfigurationsdatei, lauscht standardmäßig nur auf localhost und weist
  unbekannte Optionen als Fehler zurück, statt sie zu übergehen.
- Selbsttätige Wiederverbindung im `Link`: Ein abgezogenes USB-Kabel oder ein
  neu startender Node beendet den Dienst nicht mehr. Die Wartezeit zwischen
  Versuchen wächst und ist gedeckelt; ein Kommando, das während der Störung
  abgesetzt wird, wird nach der Wiederverbindung bedient statt abgewiesen.
- `Link`-Aktor in `meshdash-core`: nimmt Kommandos entgegen, ordnet die
  Antworten des Node den Anfragen zu und verteilt alles Unaufgeforderte an
  Interessenten. Bleibt ein Node stumm, läuft das Kommando in eine
  Zeitüberschreitung, statt die Warteschlange zu blockieren.
- Serieller Transport in `meshdash-transport` für einen Node am USB-Port,
  mit der am Firmware-Quellcode belegten Baudrate 115200 als Voreinstellung.
- TCP-Transport in `meshdash-transport`, mit Wiederverbindung nach
  Verbindungsabbruch. Die Rahmenbildung liegt in einem Adapter, der über
  jedem Byte-Strom arbeitet — Serial wird ihn mitbenutzen.
- `Transport`-Trait und Mock-Transport in `meshdash-transport` (Beginn von
  Schritt 3 der Roadmap). Der Mock spielt ein Skript ab und kann
  Verbindungsabbrüche nachstellen, sodass sich Wiederverbindung ohne Hardware
  prüfen lässt.
- Opcode-Tabellen in `meshdash-proto`: Kommandos, Antworten, Pushes,
  Fehlercodes und Statistiktypen als Aufzählungstypen, jeweils mit
  `Unknown`-Fallback, damit ein Node mit neuerer Firmware nichts verliert.
- Frame-Codec für Serial und TCP in `meshdash-proto` (Teil von Schritt 2 der
  Roadmap): Kodieren ausgehender Frames und ein Decoder, der Frames aus einem
  beliebig gestückelten Bytestrom zusammensetzt, Konsolenausgaben vor dem
  Marker verwirft und nach einer unplausiblen Längenangabe wieder
  aufsynchronisiert. Opcodes gibt es noch keine.
- Gerüst (Schritt 1 der Roadmap): Cargo-Workspace mit den fünf Crates
  `proto`, `transport`, `core`, `modules`, `server` samt verdrahteter
  Abhängigkeitsrichtung; Frontend-Gerüst mit React 19, Vite, TypeScript,
  Tailwind und leerer Modul-Registry; CI für Rust, Frontend und Doku-Links;
  `justfile` für die gängigen Abläufe. Noch ohne Funktionalität.
- Projektrahmen aufgesetzt: Architekturentwurf, Modulkonzept, Konventionen,
  Entwicklerdokumentation, Entscheidungsprotokoll (ADRs) und Glossar.
- Recherchestand zum MeshCore-Companion-Protokoll dokumentiert, inklusive
  verifizierter Frame-Struktur, bekannter Opcodes und offener Fragen.
- Arbeitsanweisung für KI-Agenten (`CLAUDE.md`).

[Unreleased]: https://github.com/Jarod1230/MeshDash/commits/main

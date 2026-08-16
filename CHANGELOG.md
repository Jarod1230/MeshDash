# Changelog

Alle nennenswerten Änderungen an MeshDash werden hier festgehalten.

Format nach [Keep a Changelog](https://keepachangelog.com/de/1.1.0/),
Versionierung nach [Semantic Versioning](https://semver.org/lang/de/).

Solange die Hauptversion `0` ist, können sich APIs und Datenbankschema in
jedem Minor-Release ändern.

## [Unreleased]

### Added

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

# Changelog

Alle nennenswerten Änderungen an MeshDash werden hier festgehalten.

Format nach [Keep a Changelog](https://keepachangelog.com/de/1.1.0/),
Versionierung nach [Semantic Versioning](https://semver.org/lang/de/).

Solange die Hauptversion `0` ist, können sich APIs und Datenbankschema in
jedem Minor-Release ändern.

## [Unreleased]

### Added

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

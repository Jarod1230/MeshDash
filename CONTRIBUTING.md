# Mitarbeit an MeshDash

Danke für dein Interesse. Dieses Dokument beschreibt, wie hier gearbeitet wird.

## Sprache

- **Dokumentation, Issues, Pull-Request-Beschreibungen: Deutsch.**
- **Code, Bezeichner, Code-Kommentare, Commit-Messages, Log-Ausgaben: Englisch.**

Der Grund für die Trennung steht in [ADR-0004](docs/decisions/0004-dokumentationssprache.md).
Wer das ändern möchte: über ein Issue, nicht per Pull Request.

## Bevor du anfängst

1. Lies [`docs/architecture.md`](docs/architecture.md) — vor allem den Abschnitt
   „Was MeshDash *nicht* ist".
2. Wenn du ein Feature bauen willst: öffne zuerst ein Issue mit der Vorlage
   *Modul-Vorschlag*. Features werden in MeshDash als Module gebaut, und die
   Zuschnittsfrage klärt man besser vorher als im Review.
3. Wenn du eine Architekturentscheidung triffst oder änderst, gehört sie als ADR
   nach [`docs/decisions/`](docs/decisions/). Auch wenn sie klein wirkt.

## Ablauf

1. Branch von `main` erstellen. Namensschema in [`docs/conventions.md`](docs/conventions.md).
2. Änderungen committen — Conventional Commits, siehe ebenfalls `conventions.md`.
3. Pull Request gegen `main` öffnen. Die PR-Vorlage bitte ausfüllen, nicht löschen.
   **Das gilt ausnahmslos**, auch für Doku-Änderungen und Einzeiler: Die PRs sind
   die Übersicht darüber, was im Projekt passiert ist. Ein lokal durchgereichter
   Commit taucht dort nicht auf.
4. Was in `main` landet, muss bauen und die Tests bestehen. Sobald es CI gibt,
   ist das Pflicht-Gate.

## Was in einen Pull Request gehört

- Eine abgeschlossene Sache. Kein „und nebenbei noch schnell".
- Tests für neue Logik. Für Protokoll-Parsing sind Tests nicht verhandelbar —
  siehe unten.
- Dokumentation, die zur Änderung passt: neues Modul → Eintrag in
  `docs/module-system.md`; neue Konfigurationsoption → `docs/configuration.md`;
  Architekturentscheidung → ADR.
- Ein Eintrag in [`CHANGELOG.md`](CHANGELOG.md) unter `[Unreleased]`, wenn die
  Änderung für Nutzer sichtbar ist.

## Sonderregel: MeshCore-Protokoll

Das Companion-Protokoll ist die Stelle, an der dieses Projekt am leichtesten
kaputtgeht, weil Fehler dort still sind — ein falsch geratener Opcode liefert
keine Exception, sondern falsche Daten.

Deshalb gilt:

- **Keine Opcodes, Feldbreiten oder Offsets raten.** Jeder Wert braucht eine
  Quelle: die Upstream-Dokumentation, den Firmware-Quellcode oder eine
  Referenzimplementierung. Die Quelle kommt als Kommentar an den Wert.
- **Was nicht verifiziert ist, wird als unverifiziert markiert** und nicht
  stillschweigend als Wahrheit behandelt.
- Unbekannte Opcodes müssen durchgereicht werden können, nicht verworfen.
- Der Stand der Recherche liegt in
  [`docs/research/meshcore-companion-protocol.md`](docs/research/meshcore-companion-protocol.md).
  Neue Erkenntnisse gehören dort hinein — mit Quelle und Datum.

## Wenn du etwas gelernt hast

[`docs/lessons-learned.md`](docs/lessons-learned.md) ist kein Ziersammelband.
Wenn dich etwas eine Stunde gekostet hat, kostet es die nächste Person auch —
schreib es auf. Das gilt ausdrücklich auch für KI-Agenten, die an diesem
Repository arbeiten.

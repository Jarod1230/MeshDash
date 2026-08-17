# Was ändert sich

<!-- Kurz und in ganzen Sätzen. Was tut dieser PR und warum? -->

Behebt #

## Art der Änderung

<!-- Mehrfachnennung ist der Normalfall, nicht die Ausnahme: Angekreuzt wird,
     was im Diff steckt, nicht der Hauptzweck des PRs.

     "Dokumentation" gilt, sobald Dokumentation **inhaltlich** geändert wurde —
     ein neuer Absatz in `architecture.md`, eine überarbeitete
     `configuration.md`, ein Eintrag in `lessons-learned.md`, eine neue
     Glossar-Definition. Also alles, was jemand beim Review lesen sollte.

     Nicht angekreuzt bei reiner Pflege: einen Haken in `roadmap.md` setzen,
     eine Zeile ins `CHANGELOG.md` schreiben, eine Standspalte aktualisieren.
     Das fällt laut `CLAUDE.md` in fast jedem PR an und sagt nichts darüber,
     worauf zu schauen ist. -->

- [ ] Neues Feature bzw. neues Modul
- [ ] Fehlerbehebung
- [ ] Refactoring ohne Verhaltensänderung
- [ ] Dokumentation
- [ ] Build, CI oder Werkzeuge

## Wie geprüft

<!-- Welche Tests laufen? Gegen echte Hardware oder gegen den Mock-Transport?
     Bei Hardware bitte Firmware-Version und Gerät angeben. -->

## Checkliste

- [ ] Konventionen aus `docs/conventions.md` eingehalten
- [ ] Tests ergänzt; bei Protokoll-Parsing mit festen Byte-Arrays
- [ ] Bei Architekturentscheidungen: ADR unter `docs/decisions/` ergänzt
- [ ] Bei neuem Modul: Tabelle in `docs/module-system.md` gepflegt
- [ ] Bei neuer Konfiguration: `docs/configuration.md` gepflegt
- [ ] Bei nutzersichtbarer Änderung: `CHANGELOG.md` unter `[Unreleased]` ergänzt
- [ ] Keine Zugangsdaten, Tokens oder personenbezogenen Daten im Diff

## Bei Änderungen am Protokoll

<!-- Nur ausfüllen, wenn meshdash-proto betroffen ist. -->

- [ ] Jeder neue oder geänderte Opcode, Offset und jede Feldbreite hat eine
      **Quellenangabe im Code** — geraten wurde nichts
- [ ] Verifikationsstufe in `docs/research/meshcore-companion-protocol.md`
      aktualisiert
- [ ] Unbekannte Opcodes werden weiterhin durchgereicht, nicht verworfen

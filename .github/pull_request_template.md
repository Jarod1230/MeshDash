# Was ändert sich

<!-- Kurz und in ganzen Sätzen. Was tut dieser PR und warum? -->

Behebt #

## Art der Änderung

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

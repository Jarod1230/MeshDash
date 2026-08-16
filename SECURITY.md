# Sicherheit

## Schwachstellen melden

Bitte **keine** öffentlichen Issues für Sicherheitslücken. Nutze stattdessen
[GitHub Security Advisories](https://github.com/Jarod1230/MeshDash/security/advisories/new)
oder schreibe an den Repository-Inhaber.

Bitte gib an: betroffene Version bzw. Commit, Auswirkung, und wie sich das
Problem reproduzieren lässt.

## Sicherheitsmodell

Wichtig zum Verständnis, was MeshDash schützen kann und was nicht:

- **MeshDash ist eine Administrationsoberfläche.** Wer Zugriff auf die Oberfläche
  hat, kann über den angeschlossenen Node ins Mesh senden und — sobald das Modul
  existiert — Repeater fernkonfigurieren. Die Oberfläche gehört nicht ungeschützt
  ins offene Internet.
- **MeshDash terminiert keine Mesh-Verschlüsselung.** Die Ende-zu-Ende-Sicherheit
  von MeshCore-Nachrichten liegt in der Firmware. MeshDash sieht das, was der
  angeschlossene Companion-Node ihm liefert — entschlüsselte Nachrichten also
  im Klartext.
- **Die Datenbank ist unverschlüsselt.** Nachrichtenverläufe, Kontakte und
  Positionsdaten liegen im Klartext auf der Platte. Wer den Server-Zugriff hat,
  hat auch die Daten.
- **Repeater-Passwörter sind Zugangsdaten.** Werden sie für die Fernadministration
  gespeichert, sind sie im Klartext verwertbar. Wie damit umgegangen wird, ist
  eine offene Entscheidung — siehe [`docs/roadmap.md`](docs/roadmap.md).

Daraus folgt für den Betrieb: MeshDash hinter einen Reverse-Proxy mit TLS,
Authentifizierung aktiviert, und nicht auf einer Maschine, der du nicht traust.

## Unterstützte Versionen

Das Projekt hat noch kein Release. Bis zur ersten Version gilt: nur `main`.

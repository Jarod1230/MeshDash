# Nachrichtenseite (Frontend)

Entscheidung Stabschef 2026-09-06:

- Direkt und Kanäle bleiben **getrennt** (kein einheitlicher Gesprächsstream)
- Je Bereich Fäden aus Empfangenem und Gesendetem
- Senden im offenen Faden (kein globales Formular oben)
- Suche über den Listen
- Flache Empfangslisten entfallen in der UI; API dafür bleibt

Umgesetzt unter `web/src/modules/messages/`.
Adresse: `?bereich=direkt|kanaele`, `?faden=`, `?neu=1`.

Pflege: Eintrag unter `[Unreleased]` in `CHANGELOG.md` und Abhaken von Punkt 5
in `docs/roadmap.md` („Aus der Benutzung gemeldet“) gehören in denselben PR —
die Formulierungen stehen in der PR-Beschreibung, falls die großen Dateien hier
noch nachgezogen werden müssen.

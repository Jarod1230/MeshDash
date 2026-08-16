# MeshDash — Dokumentation

Einstiegspunkt in den Dokumentationsbestand. Jede Datei hat genau einen Zweck;
wenn du nicht weißt, wo etwas hingehört, ist die Antwort meistens hier.

## Verstehen

| Dokument | Inhalt |
| --- | --- |
| [`architecture.md`](architecture.md) | Zielarchitektur, Schichten, Datenfluss, Abgrenzung |
| [`module-system.md`](module-system.md) | Was ein Modul ist, was es darf, wie man eins baut |
| [`glossary.md`](glossary.md) | MeshCore- und Projektbegriffe |

## Mitarbeiten

| Dokument | Inhalt |
| --- | --- |
| [`development.md`](development.md) | Umgebung einrichten, Werkzeuge, Arbeitsabläufe |
| [`conventions.md`](conventions.md) | Code-Stil, Benennung, Branches, Commits, API-Form |
| [`configuration.md`](configuration.md) | Geplante Konfigurationsoberfläche |
| [`testing.md`](testing.md) | Teststrategie, insbesondere ohne Hardware |

## Nachvollziehen

| Dokument | Inhalt |
| --- | --- |
| [`decisions/`](decisions/) | Architecture Decision Records — warum etwas so ist |
| [`lessons-learned.md`](lessons-learned.md) | Was schiefging und was daraus folgt |
| [`research/`](research/) | Rechercheergebnisse zu Fremdsystemen, v.a. MeshCore |
| [`roadmap.md`](roadmap.md) | Was als Nächstes ansteht |

## Wohin schreibe ich was?

- **„Wir haben uns für X statt Y entschieden."** → neuer ADR in `decisions/`.
  Auch bei kleinen Entscheidungen. ADRs werden nicht nachträglich umgeschrieben,
  sondern durch neuere abgelöst.
- **„Das hat mich zwei Stunden gekostet, weil …"** → `lessons-learned.md`.
- **„Die Firmware macht an dieser Stelle tatsächlich …"** → `research/`, mit
  Quelle und Datum.
- **„So benutzt man Feature Z."** → das jeweilige Moduldokument, verlinkt aus
  `module-system.md`.
- **„Das müssten wir mal machen."** → `roadmap.md` oder ein Issue, nicht als
  `TODO` im Code vergraben.

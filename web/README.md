# web/

Frontend. **Noch leer** — entsteht in Schritt 1 der
[Roadmap](../docs/roadmap.md).

Stack laut [ADR-0001](../docs/decisions/0001-tech-stack.md): React, TypeScript,
Vite. Das gebaute Ergebnis wird ins Rust-Binary eingebettet; im
Entwicklungsmodus läuft Vite eigenständig und leitet `/api` ans Backend weiter.

## Geplante Struktur

```
web/
├── src/
│   ├── app/          Shell: Layout, Navigation, Routing, Theme
│   ├── lib/          API-Client, WebSocket, gemeinsame Hilfen
│   ├── ui/           modulunabhängige Bausteine
│   └── modules/      je Unterverzeichnis ein Modul
│       └── <modul>/  Seiten, Widgets, Typen, Manifest
└── ...
```

Jedes Frontend-Modul entspricht einem Backend-Modul gleichen Namens und
exportiert ein Manifest mit Routen, Navigationseinträgen und optionalen
Dashboard-Widgets. Registriert wird es in der Modul-Registry unter `src/modules/`.

Ein Modul zu entfernen heißt: Verzeichnis löschen und eine Zeile aus der
Registry streichen. Wenn dabei etwas anderes bricht, stimmt der Schnitt nicht —
siehe [`../docs/module-system.md`](../docs/module-system.md).

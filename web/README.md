# web/

Frontend. Das Gerüst steht und baut; **Funktionalität gibt es noch keine** —
die Modul-Registry ist leer und wird ab Schritt 6 der
[Roadmap](../docs/roadmap.md) gefüllt.

Stack laut [ADR-0001](../docs/decisions/0001-tech-stack.md): React 19,
TypeScript (strict), Vite, Tailwind v4. Das gebaute Ergebnis wird später ins
Rust-Binary eingebettet; im Entwicklungsmodus läuft Vite eigenständig und
leitet `/api` ans Backend weiter.

```bash
pnpm install
pnpm dev        # Entwicklungsserver mit Hot Reload
pnpm lint       # ESLint
pnpm typecheck  # tsc --noEmit
pnpm test       # Vitest
pnpm build      # Typprüfung + Produktionsbuild nach dist/
```

## Struktur

```
web/
├── src/
│   ├── app/          Shell: Layout, Navigation, Routing, Theme
│   ├── lib/          API-Client, WebSocket, gemeinsame Hilfen   (noch leer)
│   ├── ui/           modulunabhängige Bausteine                 (noch leer)
│   └── modules/      je Unterverzeichnis ein Modul
│       ├── index.ts  die Registry
│       └── types.ts  das Modul-Manifest
└── ...
```

Jedes Frontend-Modul entspricht einem Backend-Modul gleichen Namens und
exportiert ein Manifest mit Routen, Navigationseinträgen und optionalen
Dashboard-Widgets. Registriert wird es in der Modul-Registry unter `src/modules/`.

Ein Modul zu entfernen heißt: Verzeichnis löschen und eine Zeile aus der
Registry streichen. Wenn dabei etwas anderes bricht, stimmt der Schnitt nicht —
siehe [`../docs/module-system.md`](../docs/module-system.md).

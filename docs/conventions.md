# Konventionen

## Sprache

- **Deutsch:** Dokumentation, Issues, PR-Beschreibungen, Oberflächentexte.
- **Englisch:** Code, Bezeichner, Code-Kommentare, Commit-Messages, Log-Ausgaben,
  Fehlermeldungen im Code.

Begründung: [ADR-0004](decisions/0004-dokumentationssprache.md).

## Branches

```
<typ>/<kurzbeschreibung-mit-bindestrichen>
```

Typen: `feat`, `fix`, `docs`, `refactor`, `chore`, `test`.

Beispiele: `feat/nodes-modul`, `fix/serial-reconnect`, `docs/adr-transport`.

Direkt auf `main` wird nicht committet. Änderungen laufen über Pull Requests.

## Commits

[Conventional Commits](https://www.conventionalcommits.org/de/), auf Englisch,
im Imperativ:

```
<typ>(<scope>): <beschreibung>
```

Scope ist das Crate oder Modul ohne `meshdash-`-Präfix: `proto`, `transport`,
`core`, `server`, `web`, oder ein Modulname wie `nodes`.

```
feat(proto): add frame decoder for serial transport
fix(transport): reconnect after serial device disappears
docs(architecture): clarify module ownership of tables
test(proto): cover truncated frames
```

Ein Commit ist eine abgeschlossene Sache. Formatierungsläufe werden nicht mit
inhaltlichen Änderungen vermischt.

## Rust

- `cargo fmt` ist verbindlich; `rustfmt.toml` liegt im Repository-Wurzelverzeichnis,
  sobald der Workspace existiert.
- `cargo clippy -- -D warnings` muss sauber sein. Ein `#[allow(…)]` braucht einen
  Kommentar mit Begründung.
- Fehler mit `thiserror` in Bibliotheks-Crates, `anyhow` nur im Binary.
- Kein `unwrap()` und kein `expect()` außerhalb von Tests und Programmstart.
  Beim Start ist ein früher, klarer Abbruch besser als ein halb hochgefahrener
  Dienst.
- Öffentliche Elemente werden dokumentiert. Bei Protokollkonstanten gehört die
  **Quelle** in den Doc-Kommentar.
- `unsafe` wird nicht verwendet. Falls doch einmal unvermeidbar: eigener ADR.

## TypeScript und React

- Strikter Modus, kein `any`. Wenn ein Typ unbekannt ist, dann `unknown` mit
  ausdrücklicher Prüfung.
- Funktionskomponenten, keine Klassen.
- Serverzustand über den Query-Client, nicht in lokalem Komponentenzustand
  nachgebaut.
- Ein Modul bringt seine Typen selbst mit und exportiert sie über sein Manifest.

## HTTP-API

- Alles unter `/api/v1/`. Modulrouten unter `/api/v1/<modul>/…`.
- JSON, Feldnamen in `snake_case` — passend zu den Rust-Strukturen, dadurch
  entfällt eine Umbenennungsschicht.
- Zeitstempel als ISO-8601 mit Zeitzone (RFC 3339), UTC.
- Öffentliche Schlüssel als Hex-Zeichenketten in Kleinbuchstaben.
- Fehler mit passendem Statuscode und einem Rumpf der Form
  `{"error": {"code": "...", "message": "..."}}`.
- Listen sind seitenweise abrufbar, sobald sie unbegrenzt wachsen können.

## Datenbank

- Tabellennamen: `<modul>_<gegenstand>`, Plural — `nodes_contacts`,
  `telemetry_samples`.
- Migrationen sind fortlaufend nummeriert und werden nach dem Merge **nicht mehr
  geändert**. Korrekturen kommen als neue Migration.
- Zeitstempel als UTC.

## Dateinamen

- Rust und Verzeichnisse: `snake_case`.
- React-Komponenten: `PascalCase.tsx`. Sonstige TypeScript-Dateien: `camelCase.ts`.
- Dokumentation: `kebab-case.md`. ADRs: `NNNN-titel-mit-bindestrichen.md`.

## Was nicht ins Repository gehört

- Zugangsdaten, Tokens, Repeater-Passwörter — auch nicht in Beispieldateien.
- Datenbankdateien, Build-Artefakte, `node_modules`.
- Auskommentierter Code. Dafür gibt es die Versionsgeschichte.
- `TODO` ohne Kontext. Entweder ein Issue oder ein Eintrag in
  [`roadmap.md`](roadmap.md).

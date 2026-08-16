# .claude/

Konfiguration für [Claude Code](https://claude.com/claude-code) in diesem
Repository.

- **`settings.json`** — im Repository versioniert, gilt für alle. Erlaubt
  Bau- und Lesebefehle ohne Rückfrage und sperrt den Lesezugriff auf lokale
  Konfiguration und Datenbank (`meshdash.toml`, `.env`, `data/`), damit
  Zugangsdaten und Nachrichteninhalte nicht versehentlich im Kontext landen.
- **`settings.local.json`** — persönliche Ergänzungen, über `.gitignore`
  ausgenommen. Gehört nicht ins Repository.

Die inhaltliche Arbeitsanweisung steht nicht hier, sondern in
[`../CLAUDE.md`](../CLAUDE.md).

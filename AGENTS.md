# AGENTS.md

Die Arbeitsanweisung für KI-Agenten in diesem Repository steht vollständig in
**[`CLAUDE.md`](CLAUDE.md)**. Sie gilt unabhängig davon, welcher Agent gerade
arbeitet — bitte dort lesen, bevor du etwas änderst.

**Am Repo arbeiten gemeinsam vier Agenten** (Stabschef, Backend, Frontend,
Protokoll) — kein externes KI-System. Der Abschnitt „Wenn mehrere Agenten
gleichzeitig arbeiten" in `CLAUDE.md` ist deshalb keine Formalie: Er erlaubt
größere, kohärente PRs, verlangt aber weiter `gh pr list` / Remote-Zweige vor
dem Start, und er nennt die **vier Hotspots**, an denen paralleles Arbeiten hier
schon schiefgegangen ist — Migrationsnummern, ADR-Nummern, Registrierungslisten
sowie `CHANGELOG.md` / `docs/roadmap.md`. Zweige immer von `main` und gegen
`main` mergen (nie gegen einen anderen Feature-Zweig).

Die drei Punkte, an denen dieses Projekt am ehesten Schaden nimmt:

1. **Protokollwerte nicht raten.** Opcodes, Offsets und Feldbreiten des
   MeshCore-Companion-Protokolls brauchen eine belegbare Quelle. Falsche Werte
   werfen keinen Fehler, sie schreiben stillschweigend Müll in die Datenbank.
2. **Features sind Module, kein Kern-Code.** Siehe `docs/module-system.md`.
3. **Dokumentation Deutsch, Code Englisch.** Siehe
   `docs/decisions/0004-dokumentationssprache.md`.

Und die Einstiege, je nachdem woran du arbeitest:

| Bereich | Lies zuerst |
| --- | --- |
| Protokoll, Bytes | `docs/research/meshcore-companion-protocol.md` |
| Backend-Fachlichkeit | `docs/module-system.md` |
| Oberfläche | `docs/frontend.md` |
| Was als Nächstes | `docs/roadmap.md` |

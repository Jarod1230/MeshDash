# AGENTS.md

Die Arbeitsanweisung für KI-Agenten in diesem Repository steht vollständig in
**[`CLAUDE.md`](CLAUDE.md)**. Sie gilt unabhängig davon, welcher Agent gerade
arbeitet — bitte dort lesen, bevor du etwas änderst.

**Es arbeitet mehr als ein Agent an diesem Repository.** Der Abschnitt „Wenn
mehrere Agenten gleichzeitig arbeiten" in `CLAUDE.md` ist deshalb keine
Formalie: Er nennt die vier Stellen, an denen paralleles Arbeiten hier schon
schiefgegangen ist — Migrationsnummern, ADR-Nummern, Registrierungslisten und
Zweige, die gegen einen anderen Feature-Zweig gemergt wurden.

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

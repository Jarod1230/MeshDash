# AGENTS.md

Die Arbeitsanweisung für KI-Agenten in diesem Repository steht vollständig in
**[`CLAUDE.md`](CLAUDE.md)**. Sie gilt unabhängig davon, welcher Agent gerade
arbeitet — bitte dort lesen, bevor du etwas änderst.

Die drei Punkte, an denen dieses Projekt am ehesten Schaden nimmt:

1. **Protokollwerte nicht raten.** Opcodes, Offsets und Feldbreiten des
   MeshCore-Companion-Protokolls brauchen eine belegbare Quelle. Falsche Werte
   werfen keinen Fehler, sie schreiben stillschweigend Müll in die Datenbank.
2. **Features sind Module, kein Kern-Code.** Siehe `docs/module-system.md`.
3. **Dokumentation Deutsch, Code Englisch.** Siehe
   `docs/decisions/0004-dokumentationssprache.md`.

# ADR-0005: SQLite als einzige Datenbank

- **Status:** Angenommen
- **Datum:** 2026-08-16
- **Betrifft:** `meshdash-core`, alle Module

## Kontext

MeshDash schreibt mit, was im Mesh passiert: Kontakte, Nachrichten, Adverts,
Telemetrie. Daraus entsteht ein Verlauf, der über Jahre wachsen kann — vor
allem Telemetrie, die regelmäßig anfällt.

Die Randbedingungen sind ungewöhnlich klar: eine Instanz, ein Betreiber, ein
Mesh. Die Schreiblast ist niedrig — ein LoRa-Mesh erzeugt Ereignisse in der
Größenordnung von einigen pro Minute, nicht pro Millisekunde. Die Zielhardware
ist ein Raspberry Pi.

## Entscheidung

**SQLite** als einzige Datenbank, angesprochen über `sqlx`. Kein zusätzlicher
Datenbankdienst. Jedes Modul besitzt eigene Tabellen mit dem Präfix `<modul>_`
und bringt eigene Migrationen mit.

Kompilierzeit-geprüfte Queries (`sqlx::query!`) werden **nicht** verwendet.

## Begründung

- **Passt zur Auslieferungsentscheidung.** [ADR-0001](0001-tech-stack.md) legt
  ein einzelnes Artefakt fest. Ein separater Datenbankdienst würde das
  aufheben — dann wäre es wieder ein Betriebsaufwand statt einer Datei.
- **Die Last rechtfertigt nichts Größeres.** Ein Mesh mit einigen hundert Nodes
  erzeugt Datenmengen, für die SQLite auf einem Pi großzügig dimensioniert ist.
  Ein Server-DBMS zu betreiben wäre reine Zeremonie.
- **Sicherung ist eine Dateikopie.** Für einen Selbstbetreiber ein echter
  Vorteil gegenüber `pg_dump` und Wiederherstellungsprozeduren.
- **Tests werden trivial.** In-memory-Datenbank pro Test, Migrationen laufen
  echt durch, keine Testinfrastruktur. Siehe [`../testing.md`](../testing.md).
- **Gegen `sqlx::query!`:** Kompilierzeitprüfung setzt eine erreichbare
  Datenbank zur Bauzeit voraus. Das belastet jeden Build, jede CI und jeden
  Neueinsteiger — und kollidiert mit modulweise verteilten Migrationen, weil
  das Schema zur Bauzeit gar nicht vollständig feststeht. Die Prüfung wird
  stattdessen über Tests gegen ein echtes Schema erreicht.

## Verworfene Alternativen

**PostgreSQL.** Mehr Datentypen, echte Nebenläufigkeit beim Schreiben, mit
TimescaleDB gute Telemetrie-Verdichtung. Verworfen: Betriebsaufwand ohne
Gegenwert bei dieser Last, und es bricht die Ein-Artefakt-Entscheidung. Bleibt
die Option, falls sich die Annahmen als falsch erweisen.

**SQLite plus eine Zeitreihendatenbank für Telemetrie.** Fachlich das sauberste
Modell. Verworfen: zwei Datenbanken, zwei Sicherungswege, zwei Migrationswege —
für einen Betreiber mit einem Mesh unverhältnismäßig. Reicht SQLite für
Telemetrie nicht, ist Verdichtung die erste Antwort, nicht ein zweites System.

**Nur Dateien (JSON, CSV, Logdateien).** Kein Abhängigkeit, trivial zu
inspizieren. Verworfen: Abfragen über Zeiträume und Verknüpfungen müsste man
selbst bauen — also eine Datenbank nachbauen.

**Datenbank abstrahieren, um später wechseln zu können.** Verworfen: Eine
Abstraktion über SQL-Dialekte kostet sofort Aufwand für einen Wechsel, der
vielleicht nie kommt, und man landet beim kleinsten gemeinsamen Nenner. Wird
PostgreSQL nötig, ist die Migration eine bewusste, einmalige Arbeit — und
`sqlx` unterstützt beide.

## Konsequenzen

**Positiv:** kein Betriebsaufwand; Sicherung als Dateikopie; einfache Tests;
läuft auf schwacher Hardware.

**Negativ:** Ein einziger Schreiber. Bei parallelen Schreibvorgängen aus
mehreren Modulen ist mit `SQLITE_BUSY` zu rechnen — WAL-Modus und eine
vernünftige Timeout-Einstellung sind Pflicht, nicht Feinschliff. Keine
eingebaute Verdichtung von Zeitreihen. Netzwerkdateisysteme sind für die
Datenbankdatei ungeeignet; das gehört in die Betriebsdokumentation.

**Zu beachten:** Die Frage der Aufbewahrungsdauer für Telemetrie bleibt offen
(siehe [`../architecture.md`](../architecture.md), „Offene Punkte"). Sie wird
mit SQLite eher früher relevant als mit einer Zeitreihendatenbank. Der
Migrationsablauf muss Migrationen mehrerer Module zusammenführen, ohne dass
sich die Module gegenseitig kennen.

## Wann diese Entscheidung neu zu prüfen ist

- Wenn `SQLITE_BUSY` trotz WAL im Normalbetrieb auftritt.
- Wenn die Datenbank so wächst, dass Abfragen für das Dashboard zu langsam
  werden — dann zuerst Verdichtung prüfen, erst danach einen Systemwechsel.
- Wenn mehrere Gateways gleichzeitig unterstützt werden sollen. Das ändert die
  Annahme „ein Schreiber" grundlegend.

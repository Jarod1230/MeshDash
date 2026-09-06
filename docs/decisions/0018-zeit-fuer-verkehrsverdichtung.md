# ADR-0018: Was „vor einer Woche“ für die Verkehrsverdichtung heißt

- **Status:** Vorschlag
- **Datum:** 2026-09-06
- **Betrifft:** Modul `traffic`, Verbindungsebene der Karte, Stufe C „Zeit“
- **Ergänzt:** [ADR-0016](0016-verkehr-aufbewahren.md) — Rohverlauf mit Frist und
  bleibende Verdichtung bleiben; offen ist, wie die Verdichtung einen
  **Zeitraum** beantwortet.
- **Schließt** den offenen Punkt aus Stufe C der [Roadmap](../roadmap.md):
  „Was fehlt, ist die Frage, was ‚vor einer Woche‘ für die Verdichtung heißt —
  `traffic_links` kennt nur erste und letzte Sichtung, keinen Verlauf.“

## Kontext

Stufe C verlangt denselben **Zeitraumwähler** wie die Telemetrie und ein
**Abspielen**: dieselbe Region vor einer Woche. Die Rohdaten dafür liegen:

- `traffic_packets` hält den Paketverlauf mit Frist (`keep_days`,
  Voreinstellung 30 Tage),
- `nodes_adverts` und `nodes_route_changes` tragen Sichtungen und Wegwechsel.

Die **Verbindungsebene** speist sich heute aus `traffic_links`: je Paar
`(talker, listener, width)` nur `first_seen`, `last_seen` und Zähler `heard`.
Kein Verlauf, keine Buckets, kein Zeitfilter am Endpunkt
`GET /api/v1/traffic/links`.

Damit ist „vor einer Woche“ für die Verdichtung **undefiniert**. Ein Paar mit
`first_seen` am Tag 1 und `last_seen` am Tag 30 sieht über den ganzen Monat
aktiv aus — auch wenn dazwischen nichts lief. Umgekehrt verschwindet ein Paar,
das nur in der Vorwoche aktiv war und seither schweigt, sobald man nur nach
`last_seen` filtert.

ADR-0016 hat Zeitreihen-Aggregate bewusst verworfen („noch keine Erfahrung,
welche Aggregate gebraucht werden“). Stufe C erzwingt die Frage jetzt erneut,
weil Abspielen ohne Zeitachse an der Verdichtung scheitert.

**Oberflächen-Vorbild (nur UI):** Der Telemetrie-Zeitraumwähler (Presets von
einer Stunde bis 30 Tagen, API `?since=` / `?until=`) bleibt das Muster für die
Bedienung. Diese Entscheidung betrifft die **Datenbedeutung**, nicht die
Zeichenfläche.

## Entscheidung

**(Vorschlag — Jarod bestätigt oder korrigiert.)**

1. **„Vor einer Woche“ heißt: relativer rollierender Zeitraum**, nicht
   Kalenderwoche. Ein Fenster endet „jetzt“ (bzw. am gewählten `until`) und
   reicht `N` Tage zurück — dieselben Presets und dieselbe
   `since`/`until`-Semantik wie an den Listen.
2. **Historische Verbindungsebene innerhalb von `keep_days` wird aus dem
   Rohverlauf abgeleitet**, nicht aus `traffic_links`. Für ein gewähltes
   Fenster `[since, until]` zählen die Paare, die `traffic_packets` in diesem
   Fenster belegt — dieselbe Pfadlogik wie beim Schreiben der Verdichtung.
3. **`traffic_links` bleibt die bleibende, zeitlose Verdichtung** für „wer hat
   wen jemals gehört“ / aktuelle Karte ohne Zeitfilter. First/Last und Zähler
   ändern sich nicht.
4. **Dichte Zeit-Buckets in `traffic_links` (oder einer Nebentabelle) kommen
   erst**, wenn Abspielen über `keep_days` hinaus oder billige Abfragen ohne
   Rohscan nötig werden. Bis dahin reicht der Rohverlauf als Quelle der Wahrheit
   für Zeiträume.

## Begründung

**Der Rohverlauf ist die einzige Quelle mit echter Zeitachse.** Solange
`keep_days` das gewünschte Fenster abdeckt (30 Tage decken „vor einer Woche“
ab), entsteht keine neue Wahrheit in der Datenbank — nur eine Auswertung, die
man prüfen kann.

**First/Last als Zeitfilter lügt systematisch.** Überlappung von
`[first_seen, last_seen]` mit dem Fenster überzeichnet Lücken; Filter nur auf
`last_seen` unterzeichnet Vergangenheit. Beides wäre billig und falsch — genau
die Art stiller Fehler, die MeshDash vermeiden will.

**Kalenderwochen sind die falsche Metapher.** „Vor einer Woche“ und der
Telemetrie-Wähler sprechen relativ („letzte 7 Tage“), nicht „KW 35“.

**Keine neue Verdichtungsstufe ohne Bedarf.** ADR-0016 bleibt gültig: ein falsch
gewähltes Aggregat ist teurer als eine Frist. Buckets sind Folgearbeit, sobald
die Ableitung aus Paketen messbar zu teuer oder zu kurzlebig wird.

## Verworfene Alternativen

**A — Rollierende 7 Tage nur über `last_seen`.** Zeigt Paare, die kürzlich noch
einmal auftauchten, nicht die Region „vor einer Woche“. Ein stiller Repeater
verschwindet; ein sporadischer Kontakt wirkt dauerhaft.

**B — Kalenderwoche (Mo–So / ISO-Woche).** Passt nicht zum bestehenden
Zeitraumwähler und nicht zur Alltagssprache der Roadmap. Zeitzonen und
Wochenanfang wären zusätzliche, unnötige Entscheidungen.

**C — Fenster-Filter über Überlappung von `first_seen`…`last_seen`.** Billig
ohne Schemaänderung, aber ein Paar mit zwei Sichtungen am Monatsanfang und
-ende füllt den ganzen Monat. Für Abspielen unbrauchbar.

**D — Sofort dichte Buckets (z. B. Tag oder Stunde je Paar) statt Ableitung.**
Richtig, sobald `keep_days` nicht reicht oder der Scan zu teuer wird — jetzt
verfrüht. Würde Migration, Schreibpfad und API erweitern, bevor klar ist, welche
Bucket-Größe die Karte braucht.

**E — Nur First/Last belassen und Abspielen auf Paketanimation beschränken.**
Die Verbindungsebene bliebe zeitlos; „dieselbe Region vor einer Woche“ wäre nur
Flackern ohne belegte Wege. Unterbietet Stufe C.

## Konsequenzen

**Positiv**

- Stufe C kann den Zeitraumwähler anschließen, ohne `traffic_links` umzubauen.
- Historische Verbindungen sind innerhalb der Aufbewahrungsfrist **belegbar**
  (aus demselben Rohverlauf, den ADR-0016 schon speichert).
- Die bleibende Verdichtung bleibt klein und stabil.

**Negativ / Kosten**

- Abfragen für historische Verbindungsebenen scannen bzw. aggregieren
  `traffic_packets` (ggf. über `traffic_packet_stations`) — teurer als ein
  Tabellenscan von `traffic_links`.
- Jenseits von `keep_days` gibt es **keine** historische Verbindungsebene, bis
  Buckets nachgezogen werden.
- API und Karte müssen zwei Modi unterscheiden: zeitlos (`/links` wie heute)
  vs. zeitgebunden (neu, aus Paketen oder später aus Buckets).

**Zu beachten (noch kein Code in diesem ADR)**

- Neuer oder erweiterter Endpunkt mit `since`/`until` für Verbindungen;
  Antwortform an `HeardBy` anlehnen, Zähler nur für das Fenster.
- Karte (map-first): Zeitraumfilter und Abspielen ändern die Verbindungsebene,
  nicht die Hülle. UI-Vorbild bleibt der Telemetrie-Zeitraumwähler.
- Wenn Buckets kommen: eigene Migration, eigene Tabelle oder erweiterte PK —
  nicht `traffic_links` still umbiegen.

## Offene Fragen an Jarod

1. **Reicht Ableitung aus `traffic_packets` für v1**, solange das Fenster ≤
   `keep_days` ist — oder sollen Buckets (welche Größe: Stunde / Tag?) von
   Anfang an mitkommen?
2. Soll die **zeitlose** `/links`-Ansicht auf der Karte der Default bleiben,
   und der Zeitraum nur bei aktivem Wähler/Abspielen greifen?
3. Soll „Abspielen“ die Verbindungsebene **pro Frame neu** aus dem Fenster
   ableiten, oder reicht ein festes Fenster (z. B. rollierende 7 Tage) ohne
   Scrubber-Zwischenstände?
4. Wenn `keep_days` unter das gewünschte Abspiel-Fenster fällt: **Fenster
   kappen und sagen**, oder Aufbewahrung anheben?

## Wann diese Entscheidung neu zu prüfen ist

- Sobald Abspielen oder Zeitraumfilter spürbar langsam wird oder über
  `keep_days` hinaus soll.
- Sobald die Karte Zwischenstände feiner als „Fenster gesamt“ braucht
  (echter Scrubber über Stunden).
- Wenn ADR-0016 die Frist am Rohverlauf deutlich verkürzt — dann fehlen die
  Daten für die Ableitung.

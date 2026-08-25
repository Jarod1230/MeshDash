# ADR-0011: Die Karte ist die Leitansicht, mit Kacheln über MeshDash

- **Status:** Angenommen
- **Datum:** 2026-08-25
- **Betrifft:** Frontend-Shell, Module `nodes`, `telemetry`, `messages`;
  löst [ADR-0010](0010-karte.md) ab
- **Löst ab:** ADR-0010 — dort war die Karte eine von drei Ansichten im Modul
  `nodes`, und Kartenkacheln waren ausgeschlossen. Beides gilt nicht mehr.

## Kontext

MeshDash ist bisher nach Modulen geschnitten: eine Seite je Fachlichkeit,
Navigation oben, jede Seite mit ihren eigenen Listen. Das ist ordentlich und
beantwortet die Frage „was weiß ich über X", wenn man X schon kennt.

Der Betreiber sieht sein Mesh aber nicht als vier Listen. Er sieht eine Region
mit Knoten darin, und seine Fragen sind räumlich: Wer hört wen? Wo reißt die
Kette? Über welchen Repeater läuft der Verkehr gerade? Wo lohnt ein weiterer
Standort? Diese Fragen lassen sich aus den vorhandenen Listen beantworten —
aber nur, indem der Leser die Karte im Kopf zeichnet.

Dazu kommt, was das Protokoll längst hergibt und die Oberfläche nicht zeigt:
Traceroute mit Empfangsqualität je Station (`CMD_SEND_TRACE_PATH` →
`PUSH_CODE_TRACE_DATA`), Wegänderungen, und ein Protokoll **jedes gehörten
Pakets** mit SNR und RSSI (`PUSH_CODE_RX_LOG_DATA`).

[ADR-0010](0010-karte.md) entschied gegen Kacheln, weil MeshDash in Netzen ohne
Uplink läuft und weil jeder Kartenblick sonst einem fremden Server verrät, wo
das Mesh steht. Beide Gründe bestehen fort. Sie wiegen nur nicht mehr schwerer
als das, wogegen sie abgewogen werden: Wenn die Karte die Leitansicht ist, ist
„zwei Punkte im Leeren, 12 km auseinander" zu wenig. Ob zwischen zwei Knoten
ein Hügelzug oder eine flache Wiese liegt, ist bei LoRa die halbe Antwort.

## Entscheidung

**Die Karte wird die Leitansicht der Anwendung.** Sie ist keine Ansicht eines
Moduls mehr, sondern die Fläche, auf der die Module ihre Daten zeigen: Knoten
und ihre Wege aus `nodes`, Empfangsqualität und fremde Messwerte aus
`telemetry`, Verkehr aus `messages`. Wie die Knotenseite liest sie mehrere
öffentliche Modul-APIs — was ein Client darf und ein Modul nicht.

**Die Listen bleiben.** Die Karte tritt neben sie, nicht an ihre Stelle. Was
sich zählen, sortieren und durchsuchen lässt, tut das in einer Tabelle besser
als auf einer Fläche.

**Kacheln kommen über MeshDash, nicht direkt aus dem Browser.** Der Dienst
stellt sie unter `/api/v1/tiles/{z}/{x}/{y}` bereit und holt sie seinerseits
von der konfigurierten Quelle. Was er einmal geholt hat, legt er neben die
Datenbank.

**Ohne konfigurierte Quelle gibt es keine Kacheln** — dann zeichnet die Karte
wie bisher nur Knoten, Verbindungen und eine Maßstabsleiste. Das ist der
Auslieferungszustand.

**Gezeichnet wird mit Leaflet**, Rasterkacheln, keine WebGL-Vektorkarte.

## Begründung

**Warum der Umweg über den Dienst.** Er kostet einen Endpunkt und löst drei
Dinge auf einmal:

- *Der Kachelserver sieht das Mesh nicht.* Er sieht MeshDash, einmal je
  Kachel, nicht jeden Betrachter bei jedem Blick.
- *Der lokale Vorrat ist kein Umbau mehr, sondern ein voller Cache.* Wer die
  Region einmal durchfährt, hat sie. Ein Vorwärmen für einen Ausschnitt ist
  später ein Kommando, keine zweite Architektur.
- *Die Regeln bleiben an einer Stelle.* Nutzungsbedingungen der Kachelquelle,
  eine Kennung im `User-Agent`, eine Obergrenze für Anfragen — im Frontend
  wären das drei Stellen, die niemand pflegt.

**Warum Leaflet und nicht MapLibre.** Rasterkacheln lassen sich zwischenlagern,
indem man Dateien ablegt; Vektorkacheln brauchen zusätzlich Stil, Schriften und
Symbolbilder, und ein voller Cache wird zur Bündelfrage. Leaflet ist klein
genug, dass [ADR-0008](0008-frontend-bausteine.md) — wenige Abhängigkeiten,
eigene Bausteine — nicht gebrochen wird, und die Ebenen, die MeshDash
darüberlegt, sind ohnehin eigenes SVG.

**Warum die Karte nicht in ein Modul gehört.** Sie zeigt Daten aus drei
Modulen. Ein Modul `map` müsste deren Tabellen lesen, was
[`module-system.md`](../module-system.md) verbietet, oder sich denselben
Bestand ein drittes Mal aufbauen. Der Weg, den die Knotenseite geht, gilt
auch hier: Ein Client darf mehrere öffentliche APIs lesen.

## Verworfene Alternativen

**Bei ADR-0010 bleiben — keine Kacheln.** Bleibt die richtige Wahl für einen
Notfall-Einsatz ohne Uplink, und genau dafür bleibt sie als Betriebsart
erhalten. Als *Voreinstellung für alle* macht sie die Leitansicht ärmer als
die Listen, die sie ablösen soll.

**Kacheln direkt aus dem Browser laden.** Ein `<script>`-Einzeiler weniger
Arbeit. Dafür geht die Bounding-Box des Mesh mit jedem Betrachter an einen
fremden Server, es gibt keinen gemeinsamen Vorrat, und der lokale Kachelvorrat
wäre später ein echter Umbau statt eines vollen Caches.

**Kacheln mitliefern.** Verworfen wie in ADR-0010: Ein brauchbarer Ausschnitt
wiegt mehr als das Binary. Als *Cache* neben der Datenbank ist dieselbe Menge
in Ordnung — sie ist dann Betriebsdaten, nicht Auslieferungsumfang.

**MapLibre mit Vektorkacheln.** Schöner und beliebig zoombar, aber die
Zwischenlagerung ist ein anderes Problem als „Datei ablegen", und die
Bibliothek wiegt ein Vielfaches. Wenn Vektorkacheln später gewünscht sind, ist
das ein neuer ADR.

**Alles auf die Karte, Listen weg.** Eine Fläche ist schlecht darin, Dinge zu
zählen oder zu sortieren, und Nachrichten liest niemand auf einer Karte.

## Folgen

- Die Navigation bekommt die Karte als erste Station; die Übersicht rückt
  daneben statt davor.
- Neu: ein Endpunkt für Kacheln mitsamt Cache, eine Konfigurationsoption für
  die Quelle (Voreinstellung: keine), und ein Eintrag in
  [`configuration.md`](../configuration.md).
- Die Positionen aus der Nachbartelemetrie sollen auf die Karte. Der offene
  Punkt aus ADR-0010 bleibt bestehen und wird dringlicher: Sie gehören
  `telemetry` und brauchen den Weg über ein Ereignis nach
  [ADR-0007](0007-modul-ereignisse.md).
- **Ein Knoten ohne Position ist auf einer Karte nicht darstellbar.** Heute
  meldet kaum einer Koordinaten. Die Karte braucht deshalb einen ehrlichen
  Umgang damit — sie zeigt, wie viele Knoten sie *nicht* zeigt, statt sie
  stillschweigend wegzulassen, und der Betreiber kann eine Position von Hand
  eintragen. Das ist kein Beiwerk, sondern die Bedingung, unter der die
  Leitansicht überhaupt trägt.
- Was auf der Karte an Verkehr sichtbar wird, hängt an zwei Protokollfragen,
  die noch offen sind — Aufbau des rohen Pakets und Zuordnung der Pfad-Hashes
  zu Knoten. Sie werden **vor** der Karte geklärt, siehe
  [`roadmap.md`](../roadmap.md), Stufe A. Regel 1 gilt unverändert: Ohne Beleg
  wird kein Byte gedeutet, und was sich nicht zuordnen lässt, wird auch nicht
  gezeichnet.

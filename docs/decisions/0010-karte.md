# ADR-0010: Die Karte ist eine Ansicht in `nodes`, ohne Kartenkacheln

- **Status:** Abgelöst durch [ADR-0011](0011-karte-als-leitansicht.md)
- **Datum:** 2026-08-20
- **Betrifft:** Modul `nodes` (Frontend), Punkt „`map`" in der Roadmap

## Kontext

Die Roadmap führt `map` als eigenes Modul: „Positionen auf einer Karte". Beim
Bauen stellen sich zwei Fragen, die vorher niemand gestellt hat.

**Wem gehören die Positionen?** Sie stehen in `nodes_contacts` — ein Node meldet
sie in seinem Advert. Weitere kommen als CayenneLPP-Position aus der
Nachbartelemetrie und gehören `telemetry`. Ein eigenes Modul `map` dürfte
**keine** davon lesen: [`module-system.md`](../module-system.md) verbietet den
Zugriff auf fremde Tabellen. Es müsste sich seinen Bestand über Ereignisse
selbst aufbauen — eine dritte Kopie derselben Positionen.

**Woher kämen die Kartenkacheln?** Eine Karte im üblichen Sinn lädt Kacheln von
einem Server. [ADR-0008](0008-frontend-bausteine.md) hält fest, dass MeshDash in
Netzen ohne Uplink läuft; dort bliebe eine solche Karte grau. Das ist genau der
Einsatzfall, für den das Projekt existiert.

## Entscheidung

**Kein eigenes Modul.** Die Karte wird die dritte Ansicht im Modul `nodes`,
neben Liste und Netz. Sie beantwortet dieselbe Frage wie die anderen beiden —
wo sind die Knoten —, nur geografisch statt topologisch.

**Keine Kartenkacheln und keine Kartenbibliothek.** Gezeichnet werden die
Knoten selbst, mit einer Maßstabsleiste. Projiziert wird **lokal
äquirektangulär** — die Längengrade werden mit dem Kosinus der mittleren Breite
gestaucht. Nicht Web-Mercator: Der verzerrt Nord-Süd gegenüber Ost-West, und
dann wäre eine Maßstabsleiste in nur einer Richtung richtig. Auf der Ausdehnung
eines Mesh ist die lokale Näherung genauer als jede globale Projektion. Wer den
geografischen Zusammenhang braucht, öffnet einen Knoten über einen Link in
OpenStreetMap — ein bewusster Schritt nach draußen statt eines stillen
Nachladens.

## Begründung

Der Zuschnitt folgt der Frage, nicht dem Wort „Karte". Ein Modul beantwortet
laut `module-system.md` „genau eine fachliche Frage des Betreibers"; „wo sind
meine Knoten" ist keine andere Frage als „welche Knoten habe ich", sondern
dieselbe in anderer Darstellung. Ein eigenes Modul hätte eine dritte Kopie der
Positionen gebraucht, um eine Regel zu erfüllen, die es gar nicht schützt.

Ohne Kacheln bleibt die Karte in jedem Netz gleich brauchbar. Was ein
Mesh-Betreiber der Karte abliest, ist ohnehin **Abstand und Anordnung** —
„stehen die beiden Repeater 12 km auseinander" —, und das trägt eine
Maßstabsleiste. Der Straßenzusammenhang ist der seltenere Fall und einen Klick
nach OpenStreetMap wert.

## Verworfene Alternativen

**Eigenes Modul `map` mit eigenem Positionsbestand über den Ereignisbus** —
korrekt nach Buchstaben, aber es entstünde eine dritte Kopie derselben Daten,
die auseinanderlaufen kann. Die Regel schützt vor Kopplung, nicht vor
Darstellung.

**Leaflet oder MapLibre mit Kacheln von einem öffentlichen Server** — schöner,
solange Internet da ist. Ohne Uplink eine graue Fläche, und mit Uplink eine
Anfrage an einen fremden Server bei jedem Kartenblick, die verrät, wo das Mesh
steht. Beides ohne ausdrückliche Zustimmung des Betreibers.

**Kacheln mitliefern** — selbst ein kleiner Ausschnitt in brauchbarem Zoom
wiegt mehr als das gesamte Binary.

**Eine Weltkarte als Vektorumriss einbetten** — klein genug, aber nutzlos: Ein
LoRa-Mesh spannt zehn bis fünfzig Kilometer, und auf dieser Skala sagen
Ländergrenzen nichts.

## Folgen

- Die Modultabelle in `module-system.md` verliert die Zeile `map`; die Karte
  steht bei `nodes`.
- Positionen aus der Nachbartelemetrie erscheinen vorerst **nicht** auf der
  Karte. Sie gehören `telemetry`, und der saubere Weg dorthin wäre ein
  Ereignis nach [ADR-0007](0007-modul-ereignisse.md). Das ist offen und in der
  Roadmap vermerkt.
- Sollten Kacheln später doch gewünscht sein, ist das ein neuer ADR, der diesen
  ablöst — samt der Frage, wohin die Anfragen gehen.

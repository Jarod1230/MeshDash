# ADR-0020: Kein Vorwärmen von Kacheln

- **Status:** Angenommen
- **Datum:** 2026-09-19
- **Betrifft:** Modul `tiles`, Stufe D der Roadmap
- **Ändert:** den Ausblick „Ein Vorwärmen für einen Ausschnitt ist später ein
  Kommando" aus [ADR-0011](0011-karte-als-leitansicht.md). Alles Übrige dort —
  Kacheln über den Dienst, der Cache als Dateibaum, die Regeln an einer
  Stelle — gilt unverändert.

## Kontext

ADR-0011 hat den Kachel-Cache so geschnitten, dass sich ein Ausschnitt später
vorab füllen ließe: für einen Einsatz, bei dem es vor Ort keinen Uplink gibt.
Die Roadmap führte das als Stufe-D-Punkt „Kachelvorrat vorwärmen".

Vor dem Bau stand die Frage, gegen welche Quelle es laufen würde. Betrieben
wird MeshDash mit den Kacheln von OpenStreetMap (`tile.openstreetmap.org`). Die
[Tile Usage Policy](https://operations.osmfoundation.org/policies/tiles/) der
OSM Foundation, gelesen am 2026-09-19, verbietet genau diesen Anwendungsfall:

- Massenabruf ist jedes vorausgreifende Holen von Kacheln, die niemand gerade
  ansieht — ausdrücklich genannt ist das Vorab-Füllen einer Gegend.
- Offline-Nutzung ist auf `tile.openstreetmap.org` nicht erlaubt; „Gegend für
  später speichern" steht als Beispiel dabei.
- Solche Muster werden ohne Vorwarnung gesperrt, wiederholt auch dauerhaft.

## Entscheidung

MeshDash bekommt kein Vorwärmen. Der Cache füllt sich weiterhin nur mit dem,
was jemand auf der Karte tatsächlich angesehen hat. Der Punkt wird aus der
Roadmap gestrichen, nicht zurückgestellt.

## Begründung

Gegen die Quelle, die tatsächlich benutzt wird, wäre das Feature ein
Regelverstoß mit angekündigter Sperre. Eine Sperre träfe nicht nur das
Vorwärmen, sondern die ganze Karte — MeshDash hätte sich die Kachelquelle
selbst abgeschaltet.

Was vom Nutzen bleibt, deckt der vorhandene Cache ab: Wer eine Gegend vor dem
Einsatz einmal auf der Karte ansieht, hat sie danach auf der Platte. Das ist
von der Policy gedeckt, weil jede dieser Kacheln jemand angesehen hat.

## Verworfene Alternativen

**Bauen, aber für `tile.openstreetmap.org` verweigern.** Nutzbar nur mit einer
selbst gehosteten oder dafür lizenzierten Quelle. Die gibt es hier nicht, und
eine Sperrliste nach Hostnamen ist lückenhaft: Die Regeln anderer öffentlicher
Server sind nicht geprüft, und ein Spiegel unter anderem Namen fiele durch.
Ein Feature, das für den einzigen Betreiber nicht benutzbar ist, wäre Pflege
ohne Gegenwert.

**Zurückstellen.** Hielte einen Punkt in der Warteschlange, der ohne Wechsel
der Kachelquelle nie dran ist, und jeder Agent müsste die Policy erneut lesen,
um das festzustellen.

## Konsequenzen

**Positiv:** Keine Gefahr, dass MeshDash die OSM-Kacheln für den Betreiber
sperren lässt. Der Cache bleibt, was er ist; nichts daran muss sich ändern.

**Negativ:** Für einen Einsatz ohne Uplink muss die Gegend vorher von Hand
angesehen werden, auf den Zoomstufen, die man später braucht.

**Zu beachten:** Kommentare, die das Vorwärmen als künftigen Weg nennen, sind
mit diesem ADR angepasst (`tiles/mod.rs`, `tiles/tests.rs`).

## Wann diese Entscheidung neu zu prüfen ist

Wenn MeshDash eine Kachelquelle bekommt, deren Bedingungen Vorab-Abruf erlauben
— ein selbst gehosteter Tileserver oder ein Dienst mit entsprechender Lizenz.
Dann ist das Vorwärmen wieder ein Kommando über den vorhandenen Dateibaum, wie
ADR-0011 es angelegt hat.

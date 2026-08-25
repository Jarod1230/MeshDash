# ADR-0012: Positionen stammen nur aus dem Mesh

- **Status:** Angenommen
- **Datum:** 2026-08-25
- **Betrifft:** Modul `nodes`, die Kartenansicht, Stufe B der Roadmap
- **Ändert:** die Folge „der Betreiber kann eine Position von Hand eintragen"
  aus [ADR-0011](0011-karte-als-leitansicht.md). Alles Übrige dort gilt
  unverändert.

## Kontext

Für die Karte als Leitansicht braucht MeshDash Positionen, und heute meldet
kaum ein Knoten welche. Der naheliegende Ausweg ist ein Eingabefeld: Der
Betreiber weiß, wo sein Repeater steht, und trägt es ein. Genau das war in
ADR-0011 als Folge vermerkt und wurde gebaut
([PR #66](https://github.com/Jarod1230/MeshDash/pull/66)).

Beim Ansehen wurde klar, dass das die falsche Lösung für dieses Werkzeug ist.

## Entscheidung

**Eine Position auf der Karte kommt aus dem Mesh oder gar nicht.** Quellen sind
das Advert eines Knotens und die Positionsangabe in seiner Telemetrie. Es gibt
kein Feld, in das ein Mensch Koordinaten schreibt.

**Für Knoten ohne gemeldete Position ist Triangulation der vorgesehene Weg** —
aus Empfangsqualitäten gegenüber verorteten Nachbarn geschätzt, ausdrücklich
als Schätzung gekennzeichnet. Sie steht in der Roadmap hinter der Karte, nicht
davor.

## Begründung

**MeshDash ist ein Messgerät, kein Kataster.** Was es anzeigt, ist beobachtet.
Eine von Hand gesetzte Position sieht auf der Karte genauso aus wie eine
gemessene, verhält sich aber anders: Sie altert nicht, sie widerspricht nicht,
sie wird nicht falsch, wenn jemand den Repeater versetzt. Sie ist eine
Behauptung im Bestand von Beobachtungen, und Karten laden dazu ein, so etwas
für gemessen zu halten.

**Der Aufwand wächst mit dem Mesh, der Nutzen nicht.** Fünfzig Knoten von Hand
zu verorten ist Arbeit, die niemand macht und die niemand pflegt, wenn sich
etwas ändert. Triangulation skaliert ohne Pflege — sie wird besser, je mehr
verortete Nachbarn es gibt.

**Die Karte darf leer bleiben, wenn das Mesh nichts sagt.** Das ist keine
Schwäche der Anzeige, sondern eine Aussage über das Mesh. Der ehrliche Umgang
damit steht in ADR-0011: Solange zu wenige Knoten verortet sind, zeigt die
Grundfläche die topologische Anordnung, und sie sagt, wie viele sie nicht
verorten kann. Dieser Umweg wird durch diese Entscheidung wichtiger — er ist
womöglich für lange Zeit der Normalfall statt der Anfangszustand.

## Verworfene Alternativen

**Handeintrag mit Kennzeichnung** — gebaut und wieder verworfen. Die
Kennzeichnung löst das Problem nicht: Auf einer Karte mit fünfzig Punkten liest
niemand jedem Punkt seine Herkunft an, und der Punkt sitzt trotzdem da, wo ein
Mensch ihn haben wollte.

**Handeintrag nur als Notlösung, bis Triangulation da ist** — das Vorläufige
bleibt. Wer erst hundert Positionen eingetippt hat, schaltet die Schätzung
nicht ein, die seinen Eintragungen widerspricht.

**Positionen aus einer fremden Quelle beziehen**, etwa einer Karte der
Repeater-Standorte — dieselbe Vermischung von Beobachtung und Behauptung, nur
mit einem weiteren Beteiligten.

## Folgen

- Das Modul `nodes` behält genau eine Positionsquelle: das Advert. Die
  Kontaktzeile bleibt, wie sie ist; die Tabelle für gesetzte Positionen aus
  PR #66 entsteht nicht.
- **Triangulation kommt in die Roadmap, hinter die Karte.** Mit dem
  Vorbehalt, der jetzt schon absehbar ist: Aus Empfangsqualität eine Entfernung
  zu schätzen, ist bei LoRa grob — Gelände und Antennenhöhe wirken stärker als
  die Entfernung —, und es braucht verortete Nachbarn als Anker. Eine
  geschätzte Position wird als geschätzt gezeigt, mit ihrer Unsicherheit, oder
  sie wird nicht gezeigt.
- Die Karte bleibt vorerst dünn besetzt. Das ist eingepreist.

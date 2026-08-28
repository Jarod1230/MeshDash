# ADR-0016: Verkehr wird aufbewahrt, verdichtet und irgendwann vergessen

- **Status:** Angenommen
- **Datum:** 2026-08-27
- **Betrifft:** neues Modul `traffic`, Verkehrs- und Verbindungsebene der Karte
- **Schließt** den offenen Punkt aus Stufe A der [Roadmap](../roadmap.md):
  „Bevor davon etwas in die Datenbank geht, braucht es eine Entscheidung über
  Verdichtung und Aufbewahrung."

## Kontext

Der Node meldet **jedes gehörte Paket** — `PUSH_CODE_LOG_RX_DATA` (0x88),
ungefragt, vor jeder Prüfung, auch Fremdes und Verworfenes. Das ist die
reichhaltigste Quelle, die MeshDash hat, und die einzige, aus der sich ablesen
lässt, wer wen direkt hört, ohne dass jemand etwas senden muss.

Es ist zugleich die einzige Quelle, deren Menge nicht von der Zahl der Knoten
abhängt, sondern vom Betrieb. Ein reges Mesh erzeugt mehr Zeilen als alles
andere zusammen.

Zwei Anforderungen ziehen dabei in verschiedene Richtungen:

- Die Karte soll **abspielbar** sein — „dieselbe Region vor einer Woche"
  (Stufe C). Das braucht Verlauf.
- Die Datei soll nicht unbemerkt volllaufen.

## Entscheidung

**Drei Dinge, getrennt behandelt.**

**1. Der Paketverlauf wird gespeichert, mit Frist.** Je gehörtem Paket eine
Zeile: Zeitpunkt, Routentyp, Nutzlasttyp, Pfad, Empfangsqualität, Größe.
Aufbewahrt wird `[modules.traffic] keep_days`, voreingestellt **30 Tage**;
Älteres wird regelmäßig entfernt.

**2. Die Nutzlast wird nicht gespeichert.** Sie ist verschlüsselt und geht
MeshDash nichts an. Sie wird nicht einmal durchgereicht — was nicht gespeichert
wird, kann auch nicht auslaufen.

**3. Wer wen direkt hört, wird verdichtet und bleibt.** Aus dem Pfad jedes
Pakets folgt eine Kette: Jede Station hat die vorige direkt gehört, und die
letzte wurde von diesem Node direkt gehört. Das wird als Paar mit Erst- und
Letztsichtung und einer Zählung geführt — eine Tabelle, die mit der Zahl der
**Präfixe** wächst, nicht mit dem Verkehr. Sie unterliegt keiner Frist.

## Begründung

**Die Verdichtung ist das, was man wirklich behalten will.** „Wer hört wen"
ändert sich in Tagen, nicht in Sekunden. Ein Jahr davon ist ein paar tausend
Zeilen; ein Jahr Rohverkehr sind Millionen.

**Die Frist auf dem Rohverlauf ist großzügig, weil MeshDash ein Analysewerkzeug
ist.** Wer eine Störung von vorletzter Woche nachvollziehen will, braucht die
Pakete und nicht die Zusammenfassung. Dreißig Tage decken das ab; wem das zu
viel oder zu wenig ist, stellt es um.

**Nichts zu speichern wäre die billigste und die schlechteste Wahl.** Die
Verkehrsebene wäre dann nur ein Live-Flackern, und das Abspielen aus Stufe C
gäbe es nie. Die Verbindungsebene wäre weiterhin leer — an einem echten Mesh
beobachtet, siehe [PR #78](https://github.com/Jarod1230/MeshDash/pull/78).

**Präfixe statt aufgelöster Schlüssel in der Verdichtung.** Ein Pfadeintrag ist
ein bis drei Byte eines öffentlichen Schlüssels; wer daraus beim Schreiben
einen Knoten macht, schreibt eine Vermutung in die Datenbank, die sich später
nicht mehr als solche erkennen lässt. Gespeichert wird, was ankam. Aufgelöst
wird beim Lesen, und dort gilt weiter: Passen mehrere, wird niemand benannt.

## Verworfene Alternativen

**Alles behalten, ohne Frist.** Ein Mesh, das ein Jahr läuft, hinterlässt eine
Datei, die niemand erwartet hat. Eine Voreinstellung, die still wächst, ist
keine.

**Nur verdichten, den Rohverlauf gar nicht erst schreiben.** Billig und
unumkehrbar: Was nicht geschrieben wurde, lässt sich später nicht auswerten,
und jede Frage, die beim Bauen nicht vorhergesehen war, ist für immer
unbeantwortbar.

**Verdichtungsstufen wie bei Zeitreihen** (Minuten → Stunden → Tage). Für
Messwerte richtig, hier verfrüht: Es gibt noch keine Erfahrung, welche
Aggregate gebraucht werden. Die Frist ist rückgängig zu machen, ein falsch
gewähltes Aggregat nicht.

## Folgen

- Neues Modul `traffic` mit zwei Tabellen: `traffic_packets` (mit Frist) und
  `traffic_links` (Verdichtung, bleibend).
- Neue Optionen unter `[modules.traffic]`, siehe
  [`configuration.md`](../configuration.md).
- Die Verbindungsebene bekommt damit eine Quelle, die ohne Zutun wächst — und
  die Karte einen Verlauf, auf dem das Abspielen aus Stufe C aufsetzen kann.
- **Mehrdeutige Präfixe bleiben mehrdeutig.** Die Verdichtung zählt Paare von
  Präfixen. Wie stark eine Zuordnung ist, hängt an der Breite, die der Absender
  gewählt hat — bei einem Byte ist sie schwach, und die Oberfläche sagt das.

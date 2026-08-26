# ADR-0014: Die Adresse bleibt ein Pfad

- **Status:** Angenommen
- **Datum:** 2026-08-26
- **Betrifft:** Frontend-Shell, Modul-Registry
- **Ergänzt:** [ADR-0011](0011-karte-als-leitansicht.md). Die Schichten dort
  gelten unverändert; dieser ADR ersetzt nur ihre Schreibweise in der Adresse.

## Kontext

ADR-0011 fordert zweierlei: Die Karte wird nicht verlassen und nicht neu
aufgebaut, **und** der Zustand steht in der Adresse, damit ein Link dieselbe
Ansicht öffnet wie ein Klick. Als Schreibweise nennt er Abfrageparameter —
`/?knoten=<key>`, `/?ansicht=knoten`.

Beim Bau der Hülle zeigte sich, dass die Schreibweise das eigentliche Ziel gar
nicht trägt. Dass die Karte stehen bleibt, hängt nicht an der Adresse, sondern
daran, **wo die Karte im Baum hängt**: Steht sie außerhalb der Routen, wird sie
von keinem Seitenwechsel angefasst — egal, wie die Adresse geschrieben ist.

Umgekehrt kostete die vorgeschlagene Schreibweise mehr, als sie einbrachte. Die
Modul-Registry gibt jedem Modul einen Pfad und hängt einen Platzhalter daran,
damit ein Modul eigene Unterseiten haben kann, ohne dass die Hülle sie kennt
(siehe [`module-system.md`](../module-system.md)). Mit Abfrageparametern
verschwindet dieser Vertrag: Jedes Modul müsste die Parameter selbst auslesen
und seine Unterseiten selbst auseinanderhalten.

## Entscheidung

**Der Zustand steht weiterhin in der Adresse, und zwar als Pfad.** `/` ist die
Karte, `/knoten/<key>` ist ein Knoten, `/nachrichten` sind die Gespräche.

**Die Karte liegt außerhalb der Routen.** Sie wird einmal aufgebaut und danach
nie wieder ausgehängt. Die Seiten liegen als Blende darüber; ist die Adresse
`/`, gibt es diese Blende gar nicht — nicht als verborgenes Panel.

**Die Modul-Registry bleibt unverändert.** Ein neues Modul trägt weiter genau
einen Eintrag ein und ändert sonst nichts.

## Begründung

**Das Ziel war nie die Schreibweise.** ADR-0011 will, dass der Ausschnitt einen
Ausflug in die Tiefe überlebt und dass ein Link teilbar ist. Beides leistet ein
Pfad genauso.

**Ein Pfad ist die Adresse, die Menschen lesen und tippen.** `/knoten/ee12…`
sagt, was es zeigt. `/?ansicht=knoten&knoten=ee12…` sagt dasselbe umständlicher.

**Abfrageparameter bleiben frei für das, wofür sie gemacht sind.** Der
Zeitraum und die eingeblendeten Ebenen sind Verfeinerungen einer Ansicht, keine
eigene Ansicht — die gehören später an die Adresse gehängt, ohne mit der
Wegangabe um denselben Platz zu streiten.

## Verworfene Alternativen

**Der Buchstabe von ADR-0011.** Hätte den Vertrag der Modul-Registry
aufgelöst, ohne dem eigentlichen Ziel etwas hinzuzufügen.

**Beides gleichzeitig anbieten**, Pfad und Parameter. Zwei Adressen für
dieselbe Ansicht: Links, die verschieden aussehen und dasselbe meinen, und
jede Prüfung doppelt.

## Folgen

- Die bisherige Übersicht liegt nicht mehr auf `/`, sondern auf
  `/verbindung`. `/` gehört der Karte.
- Escape schließt die Blende und führt auf `/` zurück. Die Reiter liegen über
  der Blende, damit ein Seitenwechsel nicht erst ein Schließen verlangt.
- Zeitraum und Ebenen kommen später als Abfrageparameter dazu.

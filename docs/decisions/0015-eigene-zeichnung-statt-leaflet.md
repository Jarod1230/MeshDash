# ADR-0015: Die Karte zeichnet MeshDash selbst, ohne Leaflet

- **Status:** Angenommen
- **Datum:** 2026-08-27
- **Betrifft:** Grundfläche im Frontend, Stufe C der Roadmap
- **Ändert:** die Festlegung „Gezeichnet wird mit Leaflet" aus
  [ADR-0011](0011-karte-als-leitansicht.md). Alles Übrige dort — Kacheln über
  den Dienst, Rasterkacheln, die drei Schichten — gilt unverändert.

## Kontext

ADR-0011 legte Leaflet fest, damit MeshDash nicht selbst eine Karte bauen muss.
Zu diesem Zeitpunkt gab es die Grundfläche noch nicht.

Beim Umbau der Hülle ([ADR-0014](0014-die-adresse-bleibt-ein-pfad.md)) ist sie
entstanden: eine formatfüllende SVG-Fläche mit Projektion, Ziehen, Zoomen um
den Zeiger, Maßstabsleiste und den Knoten darauf, samt Tests. Sie rechnete
lokal um den Mittelpunkt, was ohne Kacheln die genauere Wahl ist.

Kacheln erzwingen Web-Mercator. Die Frage war damit nicht mehr „selbst bauen
oder Leaflet", sondern: **die vorhandene Zeichnung umprojizieren, oder sie
wegwerfen und durch Leaflet ersetzen.**

## Entscheidung

**Die Grundfläche rechnet in Web-Mercator und legt die Kacheln selbst.** Kein
Leaflet, keine Kartenbibliothek.

Was das an eigenem Code bedeutet, ist überschaubar und steht in
`web/src/ground/projection.ts`: die Projektion auf das Einheitsquadrat, welche
Kacheln einen Ausschnitt decken, Zoom um einen Punkt, Meter je Pixel. Alles
davon ist reine Rechnung und ohne Browser prüfbar.

## Begründung

**Die Ebenen sind der eigentliche Inhalt, und sie sind eigenes SVG.** Knoten,
Verbindungen, Verkehr — nichts davon kann eine Kartenbibliothek zeichnen. Mit
Leaflet lägen sie in einem Overlay, das bei jeder Bewegung mit Leaflets
Transformation synchron gehalten werden muss. Ohne Leaflet liegen sie im selben
SVG wie die Kacheln und bewegen sich mit ihnen, weil es dieselbe Rechnung ist.

**Der Anlass für Leaflet war, die Kartenmechanik nicht schreiben zu müssen.**
Sie ist geschrieben. Was fehlte, war die Projektion — etwa vierzig Zeilen, die
gegen eine nachschlagbare Kachelnummer geprüft werden können und es auch werden.

**[ADR-0008](0008-frontend-bausteine.md) bleibt eingehalten.** Wenige
Abhängigkeiten, eigene Bausteine. Leaflet wäre die größte im Projekt gewesen.

## Was das kostet

Ehrlich benannt, weil es später auffällt:

- **Kein Pinch-Zoom auf Touchgeräten**, kein Nachlaufen beim Loslassen. Beides
  ließe sich nachrüsten; heute ist es nicht da.
- **Kein Einblenden**, während eine Kachel lädt — sie erscheint, wenn sie da
  ist.
- **Kein Umbruch an der Datumsgrenze.** Für ein Mesh, das eine Region ist,
  spielt das keine Rolle.
- **Kein Ökosystem.** Fertige Ebenen von Dritten lassen sich nicht einhängen.

Wird eines davon wichtig, ist das ein neuer ADR und kein stiller Umbau.

## Verworfene Alternativen

**Leaflet einziehen, wie ADR-0011 vorsah.** Hätte die vorhandene Arbeit
weggeworfen und die eigenen Ebenen in ein Overlay verschoben, das bei jeder
Bewegung nachgeführt werden muss — Aufwand, der bleibt, statt einmal anzufallen.

**Lokale Projektion behalten, Kacheln umrechnen.** Ginge nicht: Rasterkacheln
sind rechteckige Bilder in Mercator. Wer sie in eine andere Projektion legt,
verzieht sie oder setzt sie falsch — genau der Fehler, den eine Karte nicht
machen darf.

**Vektorkacheln mit MapLibre.** Wie in ADR-0011 verworfen: Die
Zwischenlagerung ist ein anderes Problem als „Datei ablegen", und die
Bibliothek wiegt ein Vielfaches.

## Folgen

- `web/src/ground/projection.ts` rechnet in Web-Mercator; die Kacheln liegen
  als `<image>` im selben SVG wie die Knoten.
- Der Maßstab gilt für die Bildschirmmitte und nirgends sonst. Das ist Mercator
  und steht so im Code; über die Spanne eines Mesh ist der Unterschied kleiner
  als die Leiste breit ist.
- Kacheln werden mit Token geholt und als Objekt-URL eingehängt, weil ein
  `<img>` keinen Header setzen kann und die API als Ganzes geschützt ist
  ([ADR-0006](0006-authentifizierung.md)).

# ADR-0017: Einstellungen lassen sich im Betrieb ändern

- **Status:** Angenommen
- **Datum:** 2026-08-28
- **Betrifft:** `meshdash-core`, alle Module, die Oberfläche
- **Ergänzt:** [ADR-0002](0002-modulare-architektur.md) und
  [`module-system.md`](../module-system.md) — der Vertrag eines Moduls bleibt,
  nur seine Einstellungen sind nicht mehr für die Laufzeit eingefroren.

## Kontext

Modul-Einstellungen kamen bisher ausschließlich aus `meshdash.toml` und wurden
beim Start gelesen. Wer die Nachbarabfrage einschalten wollte, brauchte
Dateizugriff und einen Neustart.

Das ist an zwei Stellen aufgeschlagen, beide vom Betreiber gemeldet: Die
Telemetrieseite verwies auf `[modules.telemetry] neighbours` — eine Option, an
die man über die Oberfläche nicht herankommt —, und die Aufbewahrungsdauer für
Pakete war ebenso unerreichbar. Eine Oberfläche, die auf eine Datei verweist,
schiebt ihre Aufgabe weiter.

## Entscheidung

**Zwei Schichten, und die obere gewinnt.** Was Datei und Umgebung sagen, ist der
Grund; er wird einmal beim Start gelesen und ändert sich nicht. Darüber liegen
die Änderungen, die jemand über die Oberfläche gemacht hat. Sie stehen in der
Datenbank.

**Option für Option, nicht Abschnitt für Abschnitt.** Eine gespeicherte Änderung
überschreibt genau die Option, die sie nennt. Alles daneben kommt weiter aus der
Datei.

**Änderungen gehen in die Datenbank, nicht in die Datei.** `meshdash.toml`
gehört dem Betreiber. MeshDash schriebe sie um, verlöre seine Kommentare und
liefe mit seinem Editor um die Wette. Was die Oberfläche ändert, ist der eigene
Zustand von MeshDash — und der gehört in dessen Datenbank.

**Nicht alles ist im Angebot.** Wo MeshDash lauscht, an welchem Gerät der Node
hängt, wo die Datenbank liegt, woher die Kacheln kommen: Das entscheidet, wie
der Prozess startet. Eine Seite, die dieser Prozess ausliefert, kann es nicht
ändern — und die Seite sagt das, statt eine Lücke zu lassen.

**Eine Änderung wird angekündigt**, als `AppEvent::SettingsChanged`. Module, die
einen Wert beim Start festhalten, reagieren darauf. **Besser ist, ihn beim
Benutzen zu lesen** — dann braucht es gar nichts.

## Begründung

**Ein Schalter, der erst nach einem Neustart wirkt, ist kein Schalter.** Wer die
Nachbarabfrage anhakt, erwartet, dass gefragt wird. `telemetry` und `traffic`
lesen ihre Einstellungen deshalb jetzt bei jeder Runde statt einmal beim Start.

**Die Datei bleibt maßgeblich für alles Unberührte.** Das ist die Bedingung
dafür, dass eine Installation reproduzierbar bleibt: Datei kopieren, gleiches
Verhalten — abzüglich dessen, was jemand bewusst umgestellt hat. Die Oberfläche
zeigt an, wo das der Fall ist.

**Geprüft wird gegen den Typ des Moduls, bevor etwas gespeichert wird.** Eine
verschriebene Option wird abgelehnt statt stillschweigend behalten — dieselbe
Regel wie `deny_unknown_fields` beim Lesen der Datei, aus demselben Grund: Wer
etwas umstellt und keine Wirkung sieht, sucht am falschen Ende.

## Verworfene Alternativen

**In die Datei zurückschreiben.** Kommentare weg, Formatierung weg, und ein
Rennen mit dem Editor des Betreibers. Eine Datei, die einem Menschen gehört,
schreibt ein Dienst nicht um.

**Nur anzeigen, nicht ändern.** Hätte die gemeldete Lücke nicht geschlossen,
sondern beschrieben.

**Jede Einstellung anbieten, auch die des Prozesses.** Ein Feld für `bind`, das
erst beim nächsten Start wirkt — oder, schlimmer, den Dienst unerreichbar macht.

**Ein eigenes Modul `settings`.** Es müsste die Abschnitte anderer Module lesen,
und genau das verbietet `module-system.md`. Einstellungen gehören allen Modulen
und keinem; deshalb liegen sie im Kern und werden vom Server angeboten.

## Folgen

- `AppContext.settings` ist jetzt ein lebender Speicher statt einer Kopie der
  Datei. Der Aufruf `settings.get::<T>("modul")` bleibt gleich.
- Neue Kerntabelle `core_settings`, migriert unter dem Namen `core`.
- Neue Endpunkte `GET /api/v1/settings` und `PUT /api/v1/settings/{modul}`.
- Neue Seite „Einstellungen".
- **Wer eine Option ergänzt, ergänzt sie an drei Stellen**: im Typ des Moduls,
  in [`configuration.md`](../configuration.md) und in der Liste auf der Seite —
  die braucht ohnehin einen Satz dazu, was die Option kostet. Siehe dort.
- Module, die einen Wert beim Start festhalten, müssen auf
  `AppEvent::SettingsChanged` reagieren oder ihn beim Benutzen lesen. `tiles`
  tut beides nicht: Quelle und Cache-Pfad bleiben Datei-Einstellungen, weil der
  Kachelclient beim Start gebaut wird.

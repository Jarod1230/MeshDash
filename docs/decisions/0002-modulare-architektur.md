# ADR-0002: Modulare Architektur mit Event-Bus

- **Status:** Angenommen
- **Datum:** 2026-08-16
- **Betrifft:** `meshdash-core`, `meshdash-modules`, `meshdash-server`, Frontend

## Kontext

MeshDash soll „eine Menge können" und über die Zeit immer mehr. Die
Standardentwicklung eines solchen Projekts ist bekannt: Features werden dort
ergänzt, wo gerade Platz ist, der Kern lernt jede Fachlichkeit mit, und nach
einem Jahr hängt alles an allem. Danach ist ein Feature nicht mehr
herauszulösen, ohne dass drei andere brechen.

Gleichzeitig darf die Modularität nicht so schwer wiegen, dass ein einzelner
Betreiber sie nicht mehr überblickt. Ein Plugin-System mit dynamischem Laden,
Versionsauflösung und stabiler ABI ist für dieses Projekt deutlich zu viel.

## Entscheidung

Fachlichkeit wird in **Modulen** organisiert. Ein Modul besitzt eigene Tabellen,
eigene HTTP-Routen und eigene Oberfläche und registriert sich zur Kompilierzeit
in einer Registry. Module kommunizieren **nicht direkt miteinander**, sondern
ausschließlich über einen Broadcast-**Event-Bus**.

Der Kern (`meshdash-core`) kennt keine Fachlichkeit. Er stellt Konfiguration,
Persistenz, Event-Bus, Transport und HTTP-Grundgerüst bereit — mehr nicht.

## Begründung

- **Der Event-Bus entkoppelt tatsächlich.** Ein Modul, das nur auf Ereignisse
  hört und in eigene Tabellen schreibt, lässt sich entfernen, ohne dass ein
  anderes es merkt. Das ist der Unterschied zwischen Modularität und
  Verzeichnisstruktur.
- **Registrierung zur Kompilierzeit statt dynamisches Laden.** Der Nutzen
  dynamischer Plugins — Erweiterung ohne Neubau — ist für ein selbst
  betriebenes Dashboard gering, die Kosten (stabile ABI, Versionsauflösung,
  Sicherheitsfragen) sind hoch. Ein Neubau ist zumutbar.
- **Tabellenbesitz erzwingt den Schnitt.** Sobald ein Modul in fremden Tabellen
  liest, entsteht eine unsichtbare Kopplung, die kein Compiler bemerkt. Die
  Regel ist streng, weil ihre Verletzung nicht auffällt.
- **Der Lackmustest ist einfach.** Ein Modul entfernen heißt: zwei
  Registrierungslisten anfassen. Geht mehr kaputt, war der Schnitt falsch.

## Verworfene Alternativen

**Dynamische Plugins (`dlopen`, WASM).** Echte Erweiterbarkeit ohne Neubau,
Module aus Fremdquellen möglich. Verworfen: erheblicher Aufwand für stabile
Schnittstellen, und Fremdcode mit Zugriff auf ein Funkgerät und die
Nachrichtendatenbank wirft Sicherheitsfragen auf, die dieses Projekt nicht
beantworten will. Könnte später auf Basis derselben Modulgrenze nachgerüstet
werden.

**Modularer Monolith ohne Event-Bus, mit direkten Aufrufen.** Einfacher zu
verstehen und zu debuggen — man sieht am Aufruf, wer wen braucht. Verworfen,
weil direkte Aufrufe eine Abhängigkeitsrichtung festschreiben. Sobald
`telemetry` das Modul `nodes` aufruft, ist `telemetry` ohne `nodes` nicht mehr
lauffähig, und die Modularität ist nur noch behauptet.

**Getrennte Dienste je Modul.** Härteste Trennung. Verworfen: völlig
unangemessen für eine Anwendung, die auf einem Raspberry Pi läuft.

**Alles im Kern, Modularisierung später.** Schnellster Start. Verworfen, weil
„später modularisieren" in der Praxis heißt: nie. Die Struktur ist am Anfang
billig und wird mit jedem Feature teurer.

## Konsequenzen

**Positiv:** Features sind einzeln entwickelbar, testbar und abschaltbar; der
Kern bleibt klein; mehrere Personen können ohne Konflikte parallel arbeiten;
neue Mitwirkende brauchen nur ein Modul zu verstehen.

**Negativ:** Indirektion. Wer wissen will, was auf ein Ereignis hin passiert,
muss die Abonnenten suchen — der Aufrufpfad steht nicht im Code. Ereignistypen
werden zu einer internen Schnittstelle, die man nicht mehr beliebig ändern kann.
Das Verbot modulübergreifender Datenbankabfragen bedeutet gelegentlich doppelt
gehaltene Daten.

**Zu beachten:** Der Kern braucht früh einen sauberen Vertrag — das
`Module`-Trait. Wird er nachlässig entworfen, wandert Fachlichkeit doch in den
Kern. Ein Ablauf für Migrationen über Modulgrenzen hinweg ist ebenfalls nötig.

## Wann diese Entscheidung neu zu prüfen ist

- Wenn sich zeigt, dass mehrere Module dieselben Daten doppelt vorhalten
  müssen, weil die Trennung zu streng ist. Dann fehlt eine Abstraktion im Kern
  — nicht die Erlaubnis für modulübergreifende Abfragen.
- Wenn der Wunsch nach Modulen aus Fremdquellen konkret wird. Dann ist die
  Frage dynamischer Plugins neu zu stellen.

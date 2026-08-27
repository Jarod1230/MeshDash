# Changelog

Alle nennenswerten Änderungen an MeshDash werden hier festgehalten.

Format nach [Keep a Changelog](https://keepachangelog.com/de/1.1.0/),
Versionierung nach [Semantic Versioning](https://semver.org/lang/de/).

Solange die Hauptversion `0` ist, können sich APIs und Datenbankschema in
jedem Minor-Release ändern.

## [Unreleased]

### Added

- **Verbindungen auf der Karte.** Linien für Wege, die tatsächlich beobachtet
  wurden: direkte Nachbarn und die Abschnitte einer Wegmessung, letztere mit
  der Empfangsqualität als Strichstärke. Was nur aus dem gespeicherten Pfad
  eines Kontakts folgen würde, wird nicht gezeichnet — dessen Stationen sind
  Ein-Byte-Präfixe, und mehr als ein Knoten kann gemeint sein. Abschaltbar über
  den Ebenenschalter unten links; die Einstellung steht in der Adresse.

- **Einen Knoten antippen, ohne die Karte zu verlassen.** Ein Klick auf einen
  Punkt öffnet eine Tafel daneben — Name, wann zuletzt gehört, Weg, Position —
  und einen Schritt weiter geht es zur vollen Knotenseite. Die Auswahl steht in
  der Adresse (`/?knoten=<schlüssel>`), ein Link öffnet also dieselbe Ansicht
  wie ein Klick. Escape schließt. Auf schmalem Schirm kommt die Tafel als
  Schublade von unten.

- **Knoten zeigen ihren Zustand.** Gefüllt heißt in der letzten Stunde gehört,
  gedämpft heute, hohl seit über einem Tag nicht mehr. Drei Zustände statt
  zwei, weil der mittlere der interessante ist: Ein Repeater, der vor einer
  Stunde noch antwortete und jetzt schweigt, ist weder in Ordnung noch weg.

- **Die Karte liegt jetzt auf Kacheln.** Ist eine Quelle konfiguriert, zeichnet
  die Grundfläche eine echte Karte unter den Knoten — in Web-Mercator, mit
  Ziehen, Zoomen und einem Maßstab, der für die Bildschirmmitte gilt. Ohne
  Quelle bleibt alles wie zuvor. Gezeichnet wird weiter selbst, ohne
  Kartenbibliothek, begründet in
  [ADR-0015](docs/decisions/0015-eigene-zeichnung-statt-leaflet.md).

  Der Ausschnitt beim Öffnen richtet sich jetzt nach den Knoten statt nach
  einer festen Zoomstufe — zwei Knoten hundert Meter auseinander bekommen eine
  Hundert-Meter-Ansicht statt einer Handvoll Pixel in der Bildmitte.

- **Kartenkacheln über MeshDash.** `GET /api/v1/tiles/{z}/{x}/{y}` holt eine
  Kachel von der konfigurierten Quelle und legt sie als Datei ab; beim zweiten
  Mal geht nichts mehr hinaus. `GET /api/v1/tiles` sagt, ob es überhaupt eine
  Quelle gibt und wem die Karte gehört.

  **Ohne `[modules.tiles] source` passiert nichts** — das ist der
  Auslieferungszustand. Wer eine Quelle nennt, muss auch die Nennung des
  Urhebers angeben, sonst startet der Dienst nicht: Die Bedingungen jedes
  Kacheldienstes verlangen sie, und eine Karte, die verschweigt, wessen sie
  ist, bringt ihren Betreiber ins Unrecht.

### Changed

- **MeshDash öffnet auf der Karte.** Die Fläche mit den Knoten darauf ist jetzt
  der Grund der Anwendung statt einer Ansicht unter „Knoten": Die Bedienung
  schwebt in den Ecken, die Seiten liegen als Blende darüber, Escape schließt
  sie wieder. Die Fläche selbst wird dabei nie neu aufgebaut — der Ausschnitt,
  auf den man gerade schaut, überlebt jeden Ausflug in eine Liste. Siehe
  [ADR-0011](docs/decisions/0011-karte-als-leitansicht.md) und
  [ADR-0014](docs/decisions/0014-die-adresse-bleibt-ein-pfad.md).

  Die Fläche richtet sich nach der Datenlage: Ab zwei gemeldeten Positionen
  zeigt sie die Geografie mit Maßstab, Ziehen und Zoomen; darunter die Ringe
  nach Zwischenstationen. Beide sagen, wie viele Knoten sie nicht verorten
  können. Die Übersichtsseite heißt jetzt „Verbindung" und liegt auf
  `/verbindung`; `/` gehört der Karte.

### Added

- **Den eigenen Node verorten.** `PUT /api/v1/system/position` sagt dem Node in
  Grad, wo er steht; er trägt es von da an in seinem Advert. Auf der
  Übersichtsseite zusammen mit dem Advert-Knopf, weil das eine ohne das andere
  im Mesh nichts ändert. Die einzige Position, die ein Mensch einträgt — und
  auch sie erscheint auf der Karte erst, wenn sie als Advert zurückkommt, siehe
  [ADR-0013](docs/decisions/0013-den-eigenen-node-verorten.md).

- **Sich dem Mesh vorstellen.** `POST /api/v1/nodes/advert` lässt den Node ein
  eigenes Advert senden — geflutet oder nur an die direkten Nachbarn. Das ist
  der Weg, von dem Mesh gefunden zu werden; es kostet Sendezeit und geschieht
  deshalb nur auf Anforderung.

### Fixed

- **„Weg unbekannt" hieß auf der Knotenseite „direkt erreichbar".** Ein Knoten,
  zu dem der Node keine Route kennt, wurde beschrieben, als läge nichts
  dazwischen. Das sind zwei verschiedene Aussagen — eine über das Mesh, die
  andere über eine Lücke im eigenen Wissen.

- **Der Node stellt sich vor.** MeshDash meldet sich beim Verbinden am Node an
  (`CMD_APP_START`) und erfährt dadurch dessen eigenen Schlüssel, Namen,
  Position, Sendeleistung und Funkparameter — Angaben, die kein anderes
  Kommando liefert. Sie stehen auf der Übersichtsseite unter „Dieser Node im
  Mesh"; die Position ist der erste sichere Punkt für die Karte.

### Fixed

- **`system` verarbeitete während einer Abfrage keine Ereignisse.** Solange die
  Geräteabfrage lief, las das Modul nichts weiter vom Bus — bei einem Node, der
  nicht antwortet, fünf Sekunden lang. Was in dieser Zeit eintraf, wurde erst
  danach oder gar nicht verarbeitet.

- **Weg messen.** Auf der Knotenseite lässt sich der bekannte Weg zu einem
  Knoten ablaufen — Station für Station, mit Empfangsqualität je Strecke.
  Das ist die belegte Quelle dafür, wie zwei *andere* Knoten einander hören;
  jede andere Messung gilt nur am eigenen Node. Kostet Sendezeit, deshalb nur
  auf Nachfrage, nie auf einem Timer. Neu ist `/api/v1/nodes/traces`.
  Unbeantwortete Versuche bleiben stehen: auch das ist ein Befund über die
  Route. Die Kartenebene daraus kommt später.

- **Positionen aus der Nachbartelemetrie auf der Karte.** Antwortet ein Knoten
  auf eine Telemetrieanfrage mit seiner Position, steht er jetzt auf der Karte,
  auch wenn sein Advert keine mitbringt — gekennzeichnet als „aus Telemetrie".
  Die Angabe aus dem Advert behält den Vorrang; die zweite Quelle füllt Lücken.

- **Pakete lesen, die das Funkmodul hört.** `meshdash_proto::packet` liest den
  unverschlüsselten Teil eines Pakets: wie es geroutet wurde, was es trägt und
  welche Stationen es weitergereicht haben. Grundlage für die Verkehrsebene der
  Karte. Noch nirgends angeschlossen — der Dienst verhält sich unverändert.

### Changed

- **Zielbild neu gefasst: MeshDash öffnet auf der Karte.** Statt vier Seiten
  nebeneinander eine Region mit den Knoten darin, formatfüllend, mit der
  Bedienung als Overlay darauf; die Listen bleiben als Blende darüber und über
  die Reiter direkt erreichbar. Damit kommen Kartenkacheln —
  ausgeliefert über MeshDash und dort zwischengelagert, damit der Kachelserver
  das Mesh nicht sieht. Ohne konfigurierte Quelle bleibt es bei der eigenen
  Zeichnung. Siehe `docs/decisions/0011-karte-als-leitansicht.md`; noch nichts
  davon umgesetzt.

### Added

- **Suchen.** Nachrichten lassen sich nach Text oder Absenderpräfix
  durchsuchen (`?q=` an der API), die Knotenliste nach Name oder Schlüssel.
  Was getippt wird, gilt wörtlich: eine Suche nach `80%` findet die Nachricht
  mit `80%` und nicht alle. Bei den Knoten filtern Liste, Netz, Karte und
  Sichtungen gemeinsam.

- **Erreichbarkeit als Band.** Die Knotenseite zeigt, wie oft ein Knoten je
  Zeitabschnitt zu hören war — hell für viel, dunkel für still. Eine Liste von
  Sichtungen beantwortet „wann gehört"; über eine Woche sind das hunderte
  Zeilen. Das Band beantwortet, wonach man sie durchsucht hätte: war er die
  ganze Zeit da, oder kommt und geht er. Zeitraum von 1 Stunde bis alles.
  Neu ist `/api/v1/nodes/presence`.

- **Wegwechsel werden aufgezeichnet.** Ändert das Mesh die Route zu einem
  Knoten, hielt die Kontaktzeile bisher nur den aktuellen Weg fest — dass sich
  etwas bewegt hat, war danach nicht mehr zu sehen. Jede Änderung wird jetzt
  mit alter und neuer Route festgehalten und steht auf der Knotenseite. Neu ist
  `/api/v1/nodes/route-changes`.

- **Zeitraum wählen.** Die Telemetrieseite zeigt nicht mehr die letzten *n*
  Messwerte, sondern eine wählbare Spanne — 1 Stunde bis 30 Tage, oder alles.
  Alle Kurven der Seite folgen derselben Wahl, sonst wären sie nicht
  vergleichbar. Die API kennt dafür `?since=` und `?until=` an allen Listen.

- **Ältere Einträge nachladen.** Nachrichten, Kanalnachrichten, die Sichtungen
  eines Knotens und der Verbindungsverlauf enden nicht mehr an einer festen
  Obergrenze — ein Knopf holt die nächstältere Seite und hängt sie unten an.
  Geblättert wird per Cursor (`?before=<id>`), nicht per Offset: Was während des
  Lesens hereinkommt, verschiebt die Seiten nicht mehr gegeneinander.

- **Eine Seite je Knoten.** Wer ist das, wie weit weg, wann zuletzt gehört, was
  wurde ausgetauscht, was meldet er über sich selbst — alles an einem Ort.
  Erreichbar aus der Knotenliste, von der Karte und aus einem Gespräch heraus.
  Aus einer Nachricht wird nur verlinkt, wenn das Sechs-Byte-Präfix eindeutig
  einem Kontakt gehört; sonst führte der Verweis womöglich zum falschen Knoten.

- **Gespräche statt Listen.** Nachrichten stehen jetzt als Faden je Kontakt und
  je Kanal — Empfangenes und Gesendetes nach Zeit verschränkt, mit
  Empfangsqualität an dem, was hereinkam, und der Sendeart an dem, was hinaus
  ging. Getrennte Listen konnten nicht zeigen, dass eine Antwort auf eine Frage
  folgte. Wer angeschrieben wurde, taucht auf, bevor er antwortet.

- **Nachrichten zeigen, von wem sie kommen.** Bisher stand dort ein
  Schlüsselpräfix wie `a1a1a1a1a1a1`; jetzt steht der Name des Knotens da,
  sofern er bekannt ist. Teilen sich mehrere bekannte Knoten dasselbe Präfix,
  wird **kein** Name gezeigt, sondern gesagt, dass es mehrdeutig ist — eine
  geratene Zuordnung wäre schlimmer als eine Hexzahl, gerade wo Nachrichten
  Anweisungen tragen können.
- **Das Protokoll ist vollständig erschlossen.** Alle 58 Kommandos der Firmware
  lassen sich bauen, alle Antworten und Meldungen lesen — von Kontakten und
  Kanälen über Funkparameter, Pfadsuche und Traceroute bis zu Signieren,
  Rohpaketen und dem Identitätsschlüssel. Genutzt wird davon bisher nichts; die
  Kommandos, die etwas zerstören oder Geheimnisse bewegen, sind an Ort und
  Stelle als solche gekennzeichnet.

- **Alle Meldungen des Node werden verstanden.** Anmeldungen an Repeatern,
  Statusantworten, Pfadänderungen, Sendebestätigungen, Traceroute-Antworten,
  Roh- und Steuerdaten, verdrängte Kontakte, voller Kontaktspeicher. Was eine
  neuere Firmware schickt und diese Fassung nicht kennt, wird vollständig
  aufbewahrt statt verworfen.

- **Die Antworten des Node lassen sich lesen.** Eigene Identität samt Schlüssel
  und Funkeinstellungen, Uhrzeit, Kennwerte, Statistiken zu Funk und Paketen,
  bekannte Routen und die Variablen angeschlossener Sensoren. Der **private**
  Schlüssel des Node wird ausdrücklich nicht gelesen.

- **Die meisten Protokollkommandos lassen sich jetzt bauen.** Zwanzig weitere
  Kommandos — Node umbenennen, Position setzen, Uhr stellen, Advert aussenden,
  Pfad zurücksetzen, Kontakt teilen oder löschen, an- und abmelden, Statistiken
  und Kennwerte abfragen, neu starten. Angeschlossen sind sie noch nicht; das
  folgt modulweise.

- **Eine Karte.** Unter „Knoten" gibt es neben Liste und Netz jetzt eine
  Kartenansicht: wer wo steht, mit Maßstabsleiste und Norden oben. Bewusst
  **ohne Kartenkacheln** — MeshDash läuft oft in Netzen ohne Uplink, wo eine
  Kachelkarte grau bliebe, und ein Kartenblick soll nicht jedes Mal einem
  fremden Server verraten, wo das Mesh steht. Wer den Straßenzusammenhang
  braucht, öffnet einen Knoten mit einem Klick in OpenStreetMap.

- **Telemetrie fremder Knoten.** MeshDash kann Nachbarn nach ihren Messwerten
  fragen — Spannung, Temperatur, Position und was sonst an Sensoren hängt — und
  zeigt sie unter „Andere Knoten". **Standardmäßig aus:** Jede Anfrage geht über
  Funk und belegt Sendezeit im Band, das sich das ganze Mesh teilt. Einschalten
  über `[modules.telemetry] neighbours = true`; gefragt wird dann ein Knoten pro
  Runde, reihum, und nur solche, die kürzlich zu hören waren.
- **Module können konfiguriert werden.** Abschnitte unter `[modules.<name>]`
  reicht der Kern an das Modul weiter, ohne sie zu deuten. Bisher hätte eine
  solche Sektion den Start sogar verhindert.

- **Grundlage für Telemetrie fremder Nodes.** MeshDash kann die Sensordaten
  lesen, die andere Knoten im CayenneLPP-Format senden — Spannung, Temperatur,
  Luftdruck, Position und mehr —, und die passende Anfrage dafür stellen.
  Genutzt wird das noch nicht; das Modul und die Anzeige folgen.

- **Die Oberfläche ist vollständig.** Alle vier Module haben eine Seite:
  Knoten mit Liste und Netzansicht, Nachrichten zum Lesen und Senden,
  Telemetrie mit Kurven für Batterie und Empfangsqualität. Die Netzansicht
  ordnet nach Zwischenstationen statt nach Orten — eine Karte wäre erfunden,
  solange kaum ein Knoten Koordinaten meldet.
- **Die Seiten aktualisieren sich von selbst.** Trifft ein Advert ein oder
  meldet der Node wartende Nachrichten, lädt die betroffene Seite nach; die
  Kopfzeile zeigt, ob der Ereignisstrom steht. Wo er nicht steht, sagt sie das
  auch — eine veraltete Seite und ein stilles Mesh sehen sonst gleich aus.
- **Nachrichten senden aus der Oberfläche**, an einen Kontakt oder in einen
  Kanal. Bei einer Direktnachricht steht dabei, ob der Node sie als Flood
  ausgesendet hat und welche Quittung er erwartet; bei einem Kanal steht, dass
  es keine gibt.

- **Eine Oberfläche.** MeshDash lässt sich zum ersten Mal bedienen statt nur
  abzufragen: Navigation aus der Modul-Registry, Hell- und Dunkelansicht, und
  eine Übersichtsseite, die zeigt, ob der Node da ist — und ob er es geblieben
  ist. Der Verbindungsverlauf läuft als Band mit, in dem jeder Abriss als Kerbe
  sichtbar bleibt; die Momentanzeige „verbunden" allein verrät nicht, dass ein
  Node alle zwei Minuten neu verbindet.
- **Verbindungsverlauf über die API** unter `/api/v1/system/connections`. Die
  Ereignisse wurden bereits aufgezeichnet, waren aber nicht abrufbar.

- **Nachrichten senden.** MeshDash schreibt zum ersten Mal etwas an den Node,
  statt ihn nur zu befragen: `POST /api/v1/messages/send` an einen Kontakt,
  `POST /api/v1/messages/channel-send` in einen Kanal. Bei einer
  Direktnachricht meldet der Node zurück, ob sie als Flood hinausging und
  welche Quittung zu erwarten ist; ein Kanal wird von niemandem quittiert und
  bekommt deshalb keine. Was hinausging, wird mitgeschrieben.
- **Kanäle.** Empfangene Kanalnachrichten stehen unter
  `/api/v1/messages/channel-received`, die Kanäle des Node unter
  `/api/v1/messages/channels`. Der gemeinsame Schlüssel eines Kanals wird
  dabei **nicht** gespeichert — wer ihn hat, kann mitlesen und mitschreiben.
- **Empfangsqualität über die Zeit** unter `/api/v1/telemetry/signal`. Jede
  empfangene Nachricht bringt einen SNR-Wert mit; daraus wird eine Zeitreihe,
  an der sich ablesen lässt, ob eine Verbindung schlechter wird.

- **Nachbarn: Adverts werden ausgewertet.** Meldet sich ein Node über Funk,
  hält MeshDash die Sichtung fest — abrufbar unter `/api/v1/nodes/adverts`,
  neueste zuerst. War der Node dem eigenen Gerät noch unbekannt, trägt die
  Meldung den vollständigen Kontakt und dieser wird gleich mit angelegt; war er
  bekannt, kommt nur sein Schlüssel und lediglich die Letztsichtung rückt vor.
  Damit ist die Kontaktliste nicht mehr die einzige Quelle: Wer gerade zu hören
  ist, steht jetzt ohne Abfrage fest.

- **Ein Binary mit eingebettetem Frontend.** `just build` legt die gebaute
  Oberfläche ins Binary; unbekannte Pfade landen bei ihr, damit deren eigene
  Navigation funktioniert, während `/api/v1/` weiterhin JSON liefert. Die
  Oberfläche selbst ist bislang eine Platzhalterseite — sie wird in Schritt 7
  der Roadmap ausgebaut.
- **Geordnetes Herunterfahren** auf Strg-C und SIGTERM: Laufende Anfragen werden
  zu Ende beantwortet, und die Verbindung zum Node wird gelöst, statt dass der
  Dienst abgeschnitten wird.
- **Live-Ereignisse über WebSocket** unter `/api/v1/events`. Verbindungsstatus
  des Node und alles, was er von sich aus meldet, erreichen den Browser ohne
  Nachfragen. Ist ein Token gesetzt, wird es als erste Nachricht erwartet —
  Browser können bei WebSocket-Verbindungen keinen Header mitgeben.
- **Authentifizierung für die API.** Ist `[auth] token` gesetzt, braucht jede
  Anfrage unter `/api/v1/` ein passendes Bearer-Token. Ist es nicht gesetzt und
  lauscht MeshDash auf einer öffentlichen Adresse, **startet der Dienst nicht** —
  wer hinter einem Reverse-Proxy ohne eigenes Token betreiben will, stimmt dem
  mit `allow_unauthenticated = true` ausdrücklich zu. Grundlage ist
  ADR-0006.
- **MeshDash startet als Dienst.** Das Binary liest die Konfiguration, legt die
  Datenbank an, baut den eingestellten Transport auf, hält die Verbindung zum
  Node selbsttätig aufrecht und lauscht auf der konfigurierten Adresse.
  Modulrouten werden unter `/api/v1/<modul>/` eingehängt; solange kein Modul
  registriert ist, antwortet der Dienst auf jeden Pfad mit `404` im
  vereinbarten Fehlerformat. Authentifizierung, WebSocket und eingebettetes
  Frontend fehlen noch.
- Auswertung eingehender Direktnachrichten in `meshdash-proto`, als Vorarbeit
  für das Modul `messages`: Absenderpräfix, Empfangsqualität, Zeitpunkt und
  Text — auch dann, wenn der Node den Text mitten im Zeichen abgeschnitten hat.
- Auswertung von Batterie- und Speicherstand des angeschlossenen Node in
  `meshdash-proto`, als Vorarbeit für das Modul `telemetry`.
- **Modul `telemetry`.** Fragt Batterie- und Speicherstand des Node alle fünf
  Minuten ab und hält den Verlauf unter `/api/v1/telemetry/battery` bereit —
  damit sichtbar wird, ob die Batterie schneller fällt als sonst. Messwerte
  anderer Nodes fehlen noch.
- **Modul `messages`.** Holt Direktnachrichten ab, sobald der Node welche
  ankündigt, und hält sie unter `/api/v1/messages/received` bereit — mit Absenderpräfix,
  Empfangsqualität und Zeitpunkt. Das Abholen leert die Warteschlange des Node,
  weshalb der hier geführte Verlauf danach die einzige Aufzeichnung ist.
  Senden und Kanäle fehlen noch.
- **Modul `nodes`.** Holt die Kontaktliste vom Node und hält sie unter
  `/api/v1/nodes/contacts` bereit — mit Name, bekanntem Pfad, Position und
  Zeitpunkten. Erst- und Letztsichtung führt MeshDash selbst, damit ein Node,
  der einen Kontakt vergisst, nicht die eigene Geschichte löscht.
- Der `Link` kann Antworten sammeln, die aus mehreren Frames bestehen — nötig
  für Abrufe wie die Kontaktliste, die als Folge von Frames beantwortet werden.
- Auswertung von Kontakten (`RESP_CODE_CONTACT`) in `meshdash-proto`, als
  Vorarbeit für das Modul `nodes`: Schlüssel, Name, bekannter Pfad, Position und
  Zeitstempel.
- **Erstes Modul: `system`.** Meldet unter `/api/v1/system/status`, ob der Node
  erreichbar ist und was er über sich sagt — Firmware, Hersteller,
  Kontaktkapazität, Kanalzahl. Jede Verbindungsänderung wird mit Zeitpunkt
  festgehalten: Dass ein Node nachts elfmal weg war, ist die eigentlich
  interessante Auskunft, und die sieht man nur im Verlauf.
- Auswertung der Node-Kennung (`RESP_CODE_DEVICE_INFO`) in `meshdash-proto` —
  die erste am Firmware-Quellcode verifizierte Nutzlast. Enthält Firmware-Stand,
  Hersteller, Kontaktkapazität und Kanalzahl.
- Modulvertrag und Registry in `meshdash-core`. Ein Modul bringt Name,
  Migrationen und einen Startvorgang mit; die Registry migriert und startet
  alle. Ein fehlschlagendes Modul verhindert den Start und wird dabei benannt.
- SQLite-Anbindung in `meshdash-core`, samt Migrationen je Modul. Die
  Datenbankdatei und ihr Verzeichnis werden beim ersten Start angelegt; jedes
  Modul zählt seine Schemaversionen unabhängig von allen anderen.
- Event-Bus in `meshdash-core`. Der `Link` meldet dort, ob der Node erreichbar
  ist und was er von sich aus schickt — die Grundlage dafür, dass mehrere
  Module dieselben Ereignisse unabhängig voneinander verarbeiten können.
- Konfiguration in `meshdash-core`: `meshdash.toml` und Umgebungsvariablen mit
  Präfix `MESHDASH_`, mit Voreinstellungen für alles. MeshDash startet ohne
  Konfigurationsdatei, lauscht standardmäßig nur auf localhost und weist
  unbekannte Optionen als Fehler zurück, statt sie zu übergehen.
- Selbsttätige Wiederverbindung im `Link`: Ein abgezogenes USB-Kabel oder ein
  neu startender Node beendet den Dienst nicht mehr. Die Wartezeit zwischen
  Versuchen wächst und ist gedeckelt; ein Kommando, das während der Störung
  abgesetzt wird, wird nach der Wiederverbindung bedient statt abgewiesen.
- `Link`-Aktor in `meshdash-core`: nimmt Kommandos entgegen, ordnet die
  Antworten des Node den Anfragen zu und verteilt alles Unaufgeforderte an
  Interessenten. Bleibt ein Node stumm, läuft das Kommando in eine
  Zeitüberschreitung, statt die Warteschlange zu blockieren.
- Serieller Transport in `meshdash-transport` für einen Node am USB-Port,
  mit der am Firmware-Quellcode belegten Baudrate 115200 als Voreinstellung.
- TCP-Transport in `meshdash-transport`, mit Wiederverbindung nach
  Verbindungsabbruch. Die Rahmenbildung liegt in einem Adapter, der über
  jedem Byte-Strom arbeitet — Serial wird ihn mitbenutzen.
- `Transport`-Trait und Mock-Transport in `meshdash-transport` (Beginn von
  Schritt 3 der Roadmap). Der Mock spielt ein Skript ab und kann
  Verbindungsabbrüche nachstellen, sodass sich Wiederverbindung ohne Hardware
  prüfen lässt.
- Opcode-Tabellen in `meshdash-proto`: Kommandos, Antworten, Pushes,
  Fehlercodes und Statistiktypen als Aufzählungstypen, jeweils mit
  `Unknown`-Fallback, damit ein Node mit neuerer Firmware nichts verliert.
- Frame-Codec für Serial und TCP in `meshdash-proto` (Teil von Schritt 2 der
  Roadmap): Kodieren ausgehender Frames und ein Decoder, der Frames aus einem
  beliebig gestückelten Bytestrom zusammensetzt, Konsolenausgaben vor dem
  Marker verwirft und nach einer unplausiblen Längenangabe wieder
  aufsynchronisiert. Opcodes gibt es noch keine.
- Gerüst (Schritt 1 der Roadmap): Cargo-Workspace mit den fünf Crates
  `proto`, `transport`, `core`, `modules`, `server` samt verdrahteter
  Abhängigkeitsrichtung; Frontend-Gerüst mit React 19, Vite, TypeScript,
  Tailwind und leerer Modul-Registry; CI für Rust, Frontend und Doku-Links;
  `justfile` für die gängigen Abläufe. Noch ohne Funktionalität.
- Projektrahmen aufgesetzt: Architekturentwurf, Modulkonzept, Konventionen,
  Entwicklerdokumentation, Entscheidungsprotokoll (ADRs) und Glossar.
- Recherchestand zum MeshCore-Companion-Protokoll dokumentiert, inklusive
  verifizierter Frame-Struktur, bekannter Opcodes und offener Fragen.
- Arbeitsanweisung für KI-Agenten (`CLAUDE.md`).

### Changed

- **Nachrichtenlisten liefern höchstens 500 Einträge** statt alles, was je
  angekommen ist. Beide Tabellen wachsen mit jeder empfangenen Nachricht und
  werden von nichts aufgeräumt; eine Abfrage hätte irgendwann den gesamten
  Verlauf in eine einzige Antwort gepackt. Mit `?limit=` lässt sich der Wert
  ändern, bis maximal 5000 — wie bei den Telemetriereihen.

### Fixed

- **Wege durchs Mesh wurden falsch gelesen.** Das Längenbyte einer Route ist
  keine Byte-Zahl, sondern trägt zwei Felder: wie viele Zwischenstationen, und
  wie breit jede ist. Dadurch zeigte MeshDash bei einem echten Node fast jeden
  Kontakt als „64 Stationen entfernt" — tatsächlich war für 22 von 25 gar kein
  Weg bekannt und einer war direkt erreichbar. Aufgefallen beim ersten Test mit
  echter Hardware. Betroffen waren Kontaktliste, Netzansicht und die
  Stationsangabe an Nachrichten.

- **Endlose Abrufschleife für Nachrichten.** Antwortete der Node auf die Frage
  nach der nächsten Nachricht dauerhaft mit etwas Unerwartetem, fragte MeshDash
  ohne Pause weiter — im Test rund 24.000 Anfragen pro Sekunde an den Funk-Node.
  Jetzt endet der Durchlauf bei einer Antwort, die weder eine Nachricht noch
  „keine weiteren" ist, und ist zusätzlich auf 500 Nachrichten je Durchlauf
  begrenzt.

[Unreleased]: https://github.com/Jarod1230/MeshDash/commits/main

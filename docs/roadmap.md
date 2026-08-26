# Roadmap

Reihenfolge der Umsetzung. Kein Terminplan — eine Abhängigkeitskette.

Grundsatz: **Von unten nach oben.** Erst das Protokoll, dann der Transport, dann
der Kern, dann Module. Umgekehrt baut man eine Oberfläche für Daten, die man
noch nicht zuverlässig lesen kann.

## Wohin das führt

**MeshDash öffnet auf der Karte.** Formatfüllend, mit der Bedienung als
Overlay darauf: eine Region mit den Knoten darin, auf der alles Weitere
sichtbar wird — wer wen hört und wie gut, worüber der Verkehr gerade läuft, wo
die Kette reißt. Von dort führt jeder Weg tiefer, oder man geht über die Reiter
direkt hinein. Drei Schichten, beschrieben in
[ADR-0011](decisions/0011-karte-als-leitansicht.md): die Karte, ein
Kontextpanel daneben, und die volle Ansicht als Blende darüber. Die Karte
bleibt dabei stehen und wird nicht neu aufgebaut.

Die Listen bleiben — als dritte Schicht, direkt über die Reiter erreichbar. Was
sich zählen, sortieren und durchsuchen lässt, tut das in einer Tabelle besser
als auf einer Fläche.

Der Weg dorthin steht unten als **Stufen A bis D**. Sie hängen aneinander:
Ohne A weiß niemand, was ein Paket überhaupt hergibt; ohne B hat die Karte zu
wenige Knoten, um eine Karte zu sein.

**Was schon steht und stehen bleibt:** die vier Module, das Blättern, die
Zeiträume, die Suche, Erreichbarkeit und Wegwechsel. Nichts davon wird
weggeworfen — die Karte ist eine zweite Tür zu denselben Daten, und ein Teil
davon ist erst durch diese Vorarbeiten überhaupt zeichenbar.

## Schritt 1 — Gerüst ✅ erledigt

Ziel war: `cargo build` und `pnpm build` laufen durch, auch wenn sie noch nichts tun.

- [x] Cargo-Workspace mit den fünf Crates aus [`architecture.md`](architecture.md),
      Abhängigkeitsrichtung verdrahtet
- [x] `rust-toolchain.toml`, `rustfmt.toml`, Workspace-Lints
      (`unsafe_code = "forbid"`, `unwrap_used`/`expect_used` als Warnung)
- [x] Frontend-Gerüst: React 19, Vite, TypeScript (strict), Tailwind v4, ESLint,
      Vitest, leere Modul-Registry
- [x] CI: Format, Clippy, Tests, Frontend-Build, Prüfung interner Doku-Links
- [x] `justfile` für die gängigen Abläufe

*Das Repository ist damit „grün" — alles Weitere hat ein Netz.*

## Schritt 2 — Protokoll (`meshdash-proto`)

Die fehleranfälligste Schicht, deshalb früh und mit Tests.

- [x] Frame-Codec: Serial-Framing kodieren und dekodieren, inklusive Teil-Frames
      — `frame::encode` und `frame::Decoder`, 20 Unit-Tests
- [x] Opcode-Tabellen mit `Unknown(u8)`-Fallback und **Quellenangabe je Wert**
      — `opcode::{Command, Response, Push, ErrorCode, StatsType}`
- Kodierung der Kommandos, Dekodierung der Antworten und Pushes, die belegt sind
- Unit-Tests gegen feste Byte-Arrays; Round-Trip-Tests
- Offene Punkte aus [`research/meshcore-companion-protocol.md`](research/meshcore-companion-protocol.md)
  abarbeiten oder ausdrücklich als offen markieren

**Vorbedingungen erfüllt (2026-08-16).** Am Firmware-Quellcode verifiziert und
belegt in [`research/meshcore-companion-protocol.md`](research/meshcore-companion-protocol.md):

- Framing für Serial und TCP — Marker-Richtung, Zählweise des Längenfelds,
  Rahmengröße, keine Prüfsumme.
- Sämtliche Opcode-Werte für Kommandos, Antworten, Pushes und Fehlercodes.

**Offen sind die Payload-Aufteilungen.** Wer ein Feld auspackt, verifiziert es
vorher einzeln — die Opcode-Liste zu kennen heißt nicht, die Nutzlast zu kennen.
Der `Unknown(u8)`-Fallback bleibt Pflicht, weil künftige Firmware mehr kennt als
diese Tabelle.

## Schritt 3 — Transport und Link

- [x] `Transport`-Trait — Frames statt Bytes, damit BLE später ohne Umbau passt
- [x] TCP über `tokio::net` — samt gemeinsamer Rahmenbildung für jeden
      Byte-Strom, die sich Serial später teilt
- [x] Serial über `tokio-serial` — teilt sich die Rahmenbildung mit TCP
- [x] Mock-Transport, der Frames aus einem Skript liefert — **gehört in diesen
      Schritt, nicht später**; inklusive nachgestellter Verbindungsabbrüche
- [x] Reconnect mit Backoff; ein abgezogenes USB-Kabel darf den Dienst nicht
      beenden — der Link verbindet selbsttätig neu, mit wachsender Wartezeit
      und Obergrenze
- [x] `Link`-Aktor: Kommando-Warteschlange, Antwortkorrelation, Push-Verteilung
      — liegt in `meshdash-core`, nicht im Transport-Crate: Pushes von Antworten
      zu unterscheiden ist Protokollwissen, und der Transport hat davon
      keines. So steht es auch im Schichtbild in
      [`architecture.md`](architecture.md).

## Schritt 4 — Kern (`meshdash-core`)

- [x] Konfiguration aus TOML und Umgebungsvariablen — `config::Config`,
      Voreinstellungen so gewählt, dass MeshDash ohne Datei startet
- [x] SQLite-Anbindung, Migrationsablauf über Modulgrenzen hinweg — `db::Database`,
      Versionsreihe je Modul, jede Migration in eigener Transaktion
- [x] Event-Bus — `event::{EventBus, AppEvent}`; der `Link` meldet dort
      Verbindungsstatus und Pushes, statt einen eigenen Verteilweg zu haben
- [x] `Module`-Trait und Registry — der Vertrag aus
      [`module-system.md`](module-system.md); Routen folgen mit Schritt 5,
      weil es vorher keinen Router gibt

## Schritt 5 — Server (`meshdash-server`)

- [x] Axum-Router, aus der Modul-Registry zusammengebaut — Modulrouten unter
      `/api/v1/<modul>/`, Fehler im Format aus
      [`conventions.md`](conventions.md); das Binary verdrahtet Konfiguration,
      Datenbank, Transport, Link und Registry und lauscht
- [x] WebSocket für Live-Ereignisse — `/api/v1/events`; das Token kommt als
      erste Nachricht, weil ein Browser dort keinen Header setzen kann
- [x] Optionale Authentifizierung — einzelnes Bearer-Token nach
      [ADR-0006](decisions/0006-authentifizierung.md); der Dienst startet nicht
      ungeschützt auf einer öffentlichen Adresse. Gilt auch für den
      Ereignisstrom.
- [x] Eingebettetes Frontend, geordnetes Herunterfahren — Frontend über das
      Merkmal `embed-frontend` im Binary (`just build`), Abbruch auf SIGINT und
      SIGTERM ohne abgeschnittene Anfragen

## Schritt 6 — Erste Module ✅ erledigt

In dieser Reihenfolge, jedes für sich abgeschlossen:

1. [x] **`system`** — Verbindungsstatus und Node-Identität, mit Verlauf jeder
   Verbindungsänderung. Bis in den Browser fehlt die Oberfläche aus Schritt 7.
2. [x] **`nodes`** — Kontakte und Nachbarn mit Verlauf.
   - [x] Kontakte abrufen, mit Erst- und Letztsichtung
   - [x] **Nachbarn** — beide Advert-Pushes ausgewertet, mit Sichtungsverlauf
         unter `/api/v1/nodes/adverts`
3. [x] **`messages`** — Direktnachrichten und Kanäle.
   - [x] Direktnachrichten empfangen, mit Verlauf
   - [x] **Senden** (`CMD_SEND_TXT_MSG`), mit Quittung des Node
   - [x] **Kanäle** — empfangen und senden, samt Kanalliste ohne Schlüssel
4. [x] **`telemetry`** — Batterie und Empfangsqualität über die Zeit.
   - [x] Batterie und Speicher des eigenen Node
   - [x] **Empfangsqualität** über die Zeit — `messages` veröffentlicht sie als
         Ereignis, `telemetry` schreibt sie fort. Siehe
         [ADR-0007](decisions/0007-modul-ereignisse.md)

**Nicht Teil dieses Schritts:** Telemetrie *fremder* Nodes
(`PUSH_CODE_TELEMETRY_RESPONSE`). Deren Nutzlast ist CayenneLPP, ein
Fremdformat — das braucht eine eigene Abhängigkeitsentscheidung und steht
unter „Danach".

## Schritt 7 — Frontend-Ausbau ✅ erledigt

- [x] **Shell** mit Modul-Registry, Navigation, Hell/Dunkel und Token-Anmeldung.
      Gestaltung und Bausteine nach
      [ADR-0008](decisions/0008-frontend-bausteine.md).
- [x] **`system`** — Verbindung, ihr Verlauf als Band und die Node-Identität.
- [x] **`nodes`** — Kontakte und Sichtungen als Liste, dazu die
      **Netzansicht**: Abstand vom Mittelpunkt heißt Zwischenstationen, die
      Richtung bedeutet nichts. Bewusst keine Karte — Koordinaten meldet kaum
      ein Knoten, und eine erfundene Anordnung sähe aus wie eine.
- [x] **`messages`** — Direktnachrichten und Kanäle, lesen und senden.
- [x] **`telemetry`** — Batterie und Empfangsqualität als Kurven in eigenem
      SVG, mit Unterbrechungen dort, wo nichts gemessen wurde.
- [x] **Live-Aktualisierung über WebSocket** — jede Seite sagt, worauf sie
      reagieren will; die Kopfzeile zeigt, ob der Strom steht.

## Danach

Nicht terminiert, nicht durchdacht — jeweils erst ein ADR, dann Code:

- [x] **Karte** — umgesetzt als dritte Ansicht im Modul `nodes`, ohne
  Kartenkacheln. Diese Entscheidung ist abgelöst: Die Karte wird die
  Leitansicht und bekommt Kacheln über MeshDash, siehe
  [ADR-0011](decisions/0011-karte-als-leitansicht.md) und „Der Weg zur Karte"
  weiter unten.
- **`admin`** und **`alerts`** — beide unter „Der Weg zur Karte", Stufe D.
- **BLE-Transport** — siehe [ADR-0003](decisions/0003-transport-priorisierung.md)
- **Mehrere Gateways gleichzeitig** — siehe „Offene Punkte" in `architecture.md`
- **Telemetrie fremder Nodes** — entschieden in
  [ADR-0009](decisions/0009-cayennelpp.md): selbst dekodieren, angefragt über
  `CMD_SEND_BINARY_REQ`. In drei Schritten, jeder für sich prüfbar:
  - [x] Dekoder und Anfrage-/Antwortkodierung in `meshdash-proto`
        (`lpp`, `binary_request`)
  - [x] Modul `telemetry` fragt Nachbarn und speichert, was zurückkommt —
        abschaltbar, standardmäßig aus, ein Knoten pro Runde
  - [x] Oberfläche: fremde Messwerte je Knoten
- **Aufbewahrung und Verdichtung von Telemetrie**
- Docker-Image, Release-Automatisierung, ARM-Builds für den Raspberry Pi

## Protokollabdeckung ✅ vollständig

Alle 58 Kommandozweige der Firmware sind baubar, sämtliche Antworten und Pushes
lesbar. Jede Nutzlast ist am Quelltext belegt, keine geraten.

**Das will gepflegt werden.** Eine neue Firmwareversion kann Felder anhängen,
Bedeutungen ändern oder Kommandos abkündigen — wie es mit
`CMD_SEND_TELEMETRY_REQ` bereits geschehen ist. Wer die Version wechselt,
vergleicht `handleCmdFrame()` und die `on…Recv()`-Methoden gegen
[`research/meshcore-companion-protocol.md`](research/meshcore-companion-protocol.md)
und zieht den Commit-Hash dort nach.

Was daraus noch nicht genutzt wird, ist der eigentliche Rückstand. Er ist im
Ausbauplan als Stufe 3 beschrieben: Aktionen an einem Knoten, den eigenen Node
einstellen, Fernadministration.

## Nutzbarkeit — Stufe 1

Aus dem Ausbauplan vom 2026-08-20:

- [x] **Absender auflösen** — `nodes` veröffentlicht seine Kontakte,
      `messages` macht daraus Namen für Sechs-Byte-Präfixe
- [x] **Gespräche statt Listen** — Verlauf je Kontakt und Kanal, Gesendetes
      und Empfangenes im selben Faden
- [x] **Knotenseite** — alles zu einem Knoten an einem Ort: Sichtungen, Weg,
      Position, Nachrichtenverlauf, gemeldete Messwerte
- [x] **Verlinkt** — aus Knotenliste, Karte und Gesprächen führt der Name zur
      Knotenseite. Aus einer Nachricht heraus nur, wenn ihr Sechs-Byte-Präfix
      eindeutig einem Kontakt gehört: Ein Verweis auf eine Vermutung öffnete
      die Seite des falschen Knotens.

Damit ist Stufe 1 erledigt. Von Stufe 2 stehen zwei Stücke:

- [x] **Blättern** — `?before=<id>` an allen wachsenden Listen, in der
      Oberfläche ein Knopf, der die nächstältere Seite anhängt.
- [x] **Zeitraum** — `?since=` und `?until=` an allen Listen; die
      Telemetrieseite wählt zwischen 1 Stunde und 30 Tagen.

- [x] **Pfadwechsel** — jede Änderung der Route zu einem Knoten wird mit alter
      und neuer Route festgehalten und steht auf der Knotenseite.

- [x] **Erreichbarkeit über die Zeit** — je Knoten ein Band aus gleich langen
      Abschnitten, das zeigt, wann er zu hören war und wann nicht.

- [x] **Suche** — Nachrichten nach Text und Absender, Knoten nach Name und
      Schlüssel.

Offen in Stufe 2: `alerts` — wartet auf die Entscheidung, wohin eine Warnung
gehen soll (nur Oberfläche, Webhook, E-Mail oder zurück ins Mesh). Es zieht
mit Stufe D um, weil eine Warnung dort hingehört, wo man sie sieht.

## Der Weg zur Karte

Vier Stufen, in dieser Reihenfolge. Jede ist für sich brauchbar; keine setzt
voraus, dass die nächste je kommt.

### Stufe A — wissen, was ein Paket hergibt ✅ erledigt

Forschung, kein Code. Sie kam zuerst, weil sie entscheidet, wie viel von
Stufe C überhaupt zeichenbar ist. Belege in
[`research/meshcore-companion-protocol.md`](research/meshcore-companion-protocol.md),
Abschnitt „Die Paketebene"; gelesen wird das Ergebnis von
`meshdash_proto::packet`.

- [x] **Aufbau des rohen Pakets** — Header mit Routentyp, Nutzlasttyp und
      Version, Transportcodes bei zwei der vier Routentypen, dann Pfad und
      verschlüsselte Nutzlast.
- [x] **Kommen diese Pushes von selbst?** Ja, und zwar für **jedes** gehörte
      Paket, vor jeder Prüfung, ohne Schalter. Auch Fremdes und Verworfenes.
      Eine Konfigurationsoption dafür wäre eine Erfindung — was MeshDash
      behält, entscheidet MeshDash.
- [x] **Zuordnung Pfad-Hash zu Knoten** — der „Hash" ist keiner: es sind die
      ersten Bytes des öffentlichen Schlüssels. Wege sind damit zuordenbar.
- [x] **Rahmen der Pfad-Antworten** — beide geklärt.

**Was daraus für die Kartenebenen folgt:**

- **Knoten** trägt. Unverändert von Positionen abhängig, das ist Stufe B.
- **Verbindungen** trägt, und besser als gedacht: Aus jedem gehörten Paket
  lässt sich ablesen, welche Stationen es weitergereicht haben — nicht nur
  aus einem Trace. Der Vorbehalt ist ein anderer als befürchtet: Ein
  Stationseintrag ist in aller Regel **ein Byte**. In einem Mesh mit einigen
  Dutzend Knoten teilen sich zwei davon mit hoher Wahrscheinlichkeit das erste
  Byte — dasselbe Geburtstagsproblem wie beim Absenderpräfix einer Nachricht,
  und dieselbe Antwort: Bei mehreren Kandidaten wird niemand benannt.
- **Verkehr** trägt, mit einer Einschränkung, die keine technische ist: Die
  Nutzlast fremder Pakete ist verschlüsselt und geht MeshDash nichts an.
  Gezeigt wird, **dass** etwas lief, welcher Art und wie gut empfangen — nicht
  was drinstand. Gespeichert wird die Nutzlast gar nicht erst.

**Offen bleibt die Menge.** Ein Rahmen je gehörtem Paket ist auf einem regen
Mesh viel. Bevor davon etwas in die Datenbank geht, braucht es eine
Entscheidung über Verdichtung und Aufbewahrung — sonst wächst die Datei
schneller als alles andere zusammen. Gehört zu Stufe C.

### Stufe B — genug Knoten, um eine Karte zu sein

Eine Karte mit zwei Punkten ist keine. Heute meldet kaum ein Knoten
Koordinaten, deshalb kommt das vor der Karte.

**Positionen kommen aus dem Mesh oder gar nicht** — kein Eingabefeld für
Koordinaten, siehe [ADR-0012](decisions/0012-positionen-nur-aus-dem-mesh.md).
Damit bleibt für diese Stufe nur, jede Quelle auszuschöpfen, die das Mesh
selbst hat.

- [x] **Positionen aus der Nachbartelemetrie auf die Karte.** Sie gehören
  `telemetry` und nehmen den Weg über ein Ereignis nach
  [ADR-0007](decisions/0007-modul-ereignisse.md) — der offene Punkt aus
  ADR-0010 ist damit erledigt. Das Advert behält den Vorrang, die zweite
  Quelle füllt Lücken.
- **Ehrlich über die Lücke.** Die Karte sagt, wie viele Knoten sie *nicht*
  zeigt, statt sie stillschweigend wegzulassen.
- [x] **Traceroute als Aktion.** `CMD_SEND_TRACE_PATH` liefert die Stationen
  eines Weges samt Empfangsqualität je Abschnitt — die einzige belegte Quelle
  für „wer hört wen wie gut" jenseits des eigenen Node. Auf der Knotenseite
  als „Weg messen", nicht auf einem Timer. Die Kartenebene daraus folgt in
  Stufe C; die Messungen sind zugleich die Datengrundlage, auf der die
  Triangulation aus Stufe D später aufsetzt.
- [x] **Den eigenen Node kennen.** `CMD_APP_START` beim Verbinden bringt
  dessen Schlüssel, Namen, Position und Funkparameter — anders nicht zu
  bekommen. Damit steht der erste sichere Punkt für die Karte fest.
- [x] **Ein eigenes Advert senden.** `POST /api/v1/nodes/advert`, geflutet oder
  nur an die Nachbarn. Am 2026-08-26 am echten Mesh ausprobiert.
- [x] **Die eigene Position setzen.** `PUT /api/v1/system/position` sagt dem
  Node in Grad, wo er steht; das Advert daneben bringt es ins Mesh. Damit
  steht der Anker, ohne den jede Schätzung frei schwebt. Die Grenze zu
  [ADR-0012](decisions/0012-positionen-nur-aus-dem-mesh.md) zieht
  [ADR-0013](decisions/0013-den-eigenen-node-verorten.md): den eigenen Node
  verortet der Betreiber, alle anderen das Mesh.

**Damit ist Stufe B erledigt.** Als Nächstes Stufe C.

### Stufe C — die Karte als Leitansicht

- [x] **Die Hülle umbauen.** MeshDash öffnet auf der Fläche; die Bedienung
  schwebt in den Ecken, die Seiten liegen als Blende darüber und Escape
  schließt sie. Die Fläche liegt außerhalb der Routen und wird nie
  ausgehängt — der Ausschnitt überlebt damit jeden Ausflug in die Tiefe. Die
  Adresse bleibt ein Pfad, begründet in
  [ADR-0014](decisions/0014-die-adresse-bleibt-ein-pfad.md).
- [x] **Grundfläche nach Datenlage.** Ab zwei gemeldeten Positionen eine
  Geografie mit Maßstab, darunter die Ringe nach Zwischenstationen. Beide
  sagen, wie viele Knoten sie nicht verorten können. Ein Punkt ist noch keine
  Geografie — er hat keine Ausdehnung, keinen Maßstab und keine Nachbarn.
- **Kartenfläche mit Kacheln über MeshDash** — Endpunkt, Plattencache,
  Konfigurationsoption; ohne Quelle bleibt es bei der eigenen Zeichnung.
- **Knotenebene** — jeder Knoten an seinem Ort, Zustand an der Farbe:
  gerade gehört, still, ausgefallen.
- **Verbindungsebene** — Linien für belegte Wege, Stärke nach
  Empfangsqualität. Was nur vermutet ist, wird nicht gezeichnet.
- **Verkehrsebene** — was gerade läuft, live über den vorhandenen
  Ereignisstrom. Wie weit sie geht, entscheidet Stufe A: von „ein Paket kam
  an, so gut war es" bis zur verfolgten Bahn über die Stationen.
- **Tiefer eintreten** — Klick auf einen Knoten öffnet das Kontextpanel; von
  dort führt ein Schritt zur vollen Knotenseite. Klick auf eine Verbindung
  zeigt ihre Geschichte, Klick auf ein Paket seinen Weg. Wer weiß, was er
  sucht, geht über die Reiter direkt hinein, ohne den Umweg über die Fläche.
- **Zeit** — derselbe Zeitraumwähler wie in der Telemetrie, dazu ein
  Abspielen: dieselbe Region vor einer Woche.

### Stufe D — handeln, wo man es sieht

- **`alerts`** — Warnung bei Ausfall; auf der Karte an dem Knoten, um den es
  geht. Braucht die Entscheidung, wohin eine Warnung sonst noch geht.
- **`admin`** — Fernadministration von Repeatern, erreichbar aus der Karte.
  Braucht vorher die Antwort auf die Frage nach den Zugangsdaten, siehe
  [`../SECURITY.md`](../SECURITY.md).
- **Kachelvorrat vorwärmen** — einen Ausschnitt einmal holen und behalten, für
  den Einsatz ohne Uplink. Nach ADR-0011 ein voller Cache, kein Umbau.
- **Knoten triangulieren.** Der vorgesehene Weg für alle Knoten, die keine
  Position melden ([ADR-0012](decisions/0012-positionen-nur-aus-dem-mesh.md)):
  aus den Empfangsqualitäten gegenüber verorteten Nachbarn geschätzt. Kommt
  bewusst spät, weil sie auf allem davor aufsetzt — verortete Anker aus Stufe B
  und gemessene Verbindungen aus Stufe C.

  Zwei Vorbehalte, jetzt schon absehbar: Aus Empfangsqualität eine Entfernung
  zu schätzen, ist bei LoRa grob — Gelände, Antennenhöhe und Sendeleistung
  wirken stärker als die Entfernung —, und ohne verortete Anker gibt es keinen
  Bezugspunkt. Eine geschätzte Position wird als geschätzt gezeigt, mit ihrer
  Unsicherheit, oder sie wird nicht gezeigt.

## Gesammelte Einfälle

Was auffällt, aber nicht dran ist. Landet hier statt als `TODO` im Code.

- **Serielle Ports auflisten — ginge ohne `libudev`.** Heute muss der Gerätepfad
  in die Konfiguration geschrieben werden, weil `tokio-serial` bewusst ohne
  dessen `libudev`-Merkmal eingebunden ist (Begründung in
  [`development.md`](development.md)). Für eine Portauswahl in der Oberfläche
  braucht es die Systembibliothek aber nicht: Unter Linux genügt ein Blick in
  `/dev/serial/by-id/`, unter macOS auf `/dev/cu.*`. Das ist ein
  Verzeichnislisting. Festgehalten, damit die Frage später nicht auf
  „`libudev` einbinden oder keine Portliste" verengt wird — es gibt einen
  dritten Weg.
- **Ein Kommando ohne Node hängt unbegrenzt.** Wer etwas sendet, während kein
  Node verbunden ist, wartet ewig: Der Antwort-Timeout im Link greift erst,
  *nachdem* ein Kommando hinausgegangen ist, und ohne Verbindung geht es gar
  nicht erst hinaus. Betrifft jeden schreibenden Endpunkt gleichermaßen —
  Nachricht senden, Weg messen, später Fernadministration. Am laufenden Dienst
  beobachtet: beide Aufrufe antworten nach zehn Sekunden noch immer nicht. Der
  saubere Weg wäre, eine Anfrage ohne Verbindung sofort abzulehnen, statt sie
  in die Warteschlange zu legen.
- **Beschriftungen auf der Karte überlagern sich** — vorerst gelöst, indem die
  Fläche einen Namen weglässt, sobald er auf einem schon gezeichneten läge, und
  darunter schreibt, wie viele sie weggelassen hat. Offen bleibt das Bessere:
  bei Enge zu einer Gruppe zusammenfassen, statt einen der beiden Namen
  auszulassen.
- **Einstellungen gehören in die Oberfläche.** Nachbarabfrage, Intervalle,
  später der Kartendienst und was sonst noch dazukommt, stehen heute in der
  Konfigurationsdatei; wer sie ändern will, braucht Dateizugriff und einen
  Neustart. Ein Einstellungsbereich, der alles an einem Ort sammelt, ist
  vorgesehen — dann auch mit dem, was der Node selbst kann (Name,
  Sendeleistung, Funkparameter).
- **Achsenbeschriftung bei winzigem Wertebereich.** Zeigt eine Kurve nur
  wenige Millivolt Unterschied, rundet die Y-Achse alle Marken auf denselben
  Text — beim Ausprobieren stand dreimal `4.10` untereinander. Die Zahl der
  Nachkommastellen müsste sich nach der Spanne richten, nicht nach der Größe.
  Beim Zeitraumwechsel aufgefallen, weil eine kurze Spanne genau das erzeugt.
- **Tabellennamen vereinheitlichen.** `conventions.md` verlangt
  `<modul>_<gegenstand>` im Plural. Vier Tabellen weichen ab:
  `messages_received`, `messages_channel_received`, `messages_sent` (Partizip
  statt Gegenstand) und `system_node_info` (Singular). Da Migrationen nach dem
  Merge nicht mehr geändert werden, bräuchte es je eine neue Migration samt
  Datenumzug — für Kosmetik zu teuer, deshalb bewusst offen gelassen und hier
  vermerkt, statt die Regel stillschweigend zu beugen.
- **Trennungsgründe maschinenlesbar machen.** `system_connection_events.reason`
  ist heute ein englischer Freitext aus der Transportschicht. Die Oberfläche
  zitiert ihn deshalb als technisches Detail unter einer deutschen Zeile, statt
  ihn als Oberflächentext auszugeben. Ein Code je Ursache — Kabel weg, Zeitüberlauf,
  abgewiesen — ließe sich übersetzen und auswerten.
- **Nachbarabfrage kommt spät in Gang.** `telemetry` erfährt von Knoten nur aus
  Adverts, weil ihm die Kontaktliste nicht gehört. Ein frisch gestartetes
  MeshDash fragt deshalb niemanden, obwohl der Node schon 25 Kontakte kennt —
  am echten Gerät beobachtet. Der saubere Weg wäre, dass `nodes` seine Kontakte
  als Ereignis veröffentlicht ([ADR-0007](decisions/0007-modul-ereignisse.md)).
- Import bestehender Verläufe aus anderen MeshCore-Clients
- Export als CSV oder für Grafana/Prometheus
- Pfadwechsel über die Zeit sichtbar machen — vermutlich das nützlichste
  Diagnosewerkzeug für Repeater-Betreiber

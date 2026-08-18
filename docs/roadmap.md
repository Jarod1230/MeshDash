# Roadmap

Reihenfolge der Umsetzung. Kein Terminplan — eine Abhängigkeitskette.

Grundsatz: **Von unten nach oben.** Erst das Protokoll, dann der Transport, dann
der Kern, dann Module. Umgekehrt baut man eine Oberfläche für Daten, die man
noch nicht zuverlässig lesen kann.

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

## Schritt 6 — Erste Module

In dieser Reihenfolge, jedes für sich abgeschlossen:

1. [x] **`system`** — Verbindungsstatus und Node-Identität, mit Verlauf jeder
   Verbindungsänderung. Bis in den Browser fehlt die Oberfläche aus Schritt 7.
2. **`nodes`** — Kontakte und Nachbarn mit Verlauf.
   - [x] Kontakte abrufen, mit Erst- und Letztsichtung
   - [ ] **Nachbarn** — Adverts (`PUSH_CODE_ADVERT`, `PUSH_CODE_NEW_ADVERT`)
         auswerten, Nutzlast dafür erst verifizieren
3. **`messages`** — Direktnachrichten und Kanäle.
   - [x] Direktnachrichten empfangen, mit Verlauf
   - [ ] **Senden** (`CMD_SEND_TXT_MSG`), Nutzlast erst verifizieren
   - [ ] **Kanäle** — empfangen und senden
4. **`telemetry`** — Batterie und Empfangsqualität über die Zeit.
   - [x] Batterie und Speicher des eigenen Node
   - [ ] **Empfangsqualität** über die Zeit. Der SNR liegt in den Nachrichten,
         die `messages` abholt; `telemetry` kommt über ein Ereignis auf dem Bus
         daran — kein CayenneLPP nötig, siehe
         [`module-system.md`](module-system.md)

**Nicht Teil dieses Schritts:** Telemetrie *fremder* Nodes
(`PUSH_CODE_TELEMETRY_RESPONSE`). Deren Nutzlast ist CayenneLPP, ein
Fremdformat — das braucht eine eigene Abhängigkeitsentscheidung und steht
unter „Danach".

## Schritt 7 — Frontend-Ausbau

- Dashboard-Shell mit Modul-Registry, Navigation, Dark/Light
- Seiten für die vier Module
- Live-Aktualisierung über WebSocket

## Danach

Nicht terminiert, nicht durchdacht — jeweils erst ein ADR, dann Code:

- **`map`** — Positionen auf einer Karte
- **`admin`** — Repeater-Fernadministration. Braucht vorher eine Antwort auf die
  Frage nach den Zugangsdaten, siehe [`../SECURITY.md`](../SECURITY.md)
- **`alerts`** — Benachrichtigung bei Node-Ausfall
- **BLE-Transport** — siehe [ADR-0003](decisions/0003-transport-priorisierung.md)
- **Mehrere Gateways gleichzeitig** — siehe „Offene Punkte" in `architecture.md`
- **Telemetrie fremder Nodes** — CayenneLPP als Fremdformat, braucht eine
  Abhängigkeitsentscheidung
- **Aufbewahrung und Verdichtung von Telemetrie**
- Docker-Image, Release-Automatisierung, ARM-Builds für den Raspberry Pi

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
- Import bestehender Verläufe aus anderen MeshCore-Clients
- Export als CSV oder für Grafana/Prometheus
- Pfadwechsel über die Zeit sichtbar machen — vermutlich das nützlichste
  Diagnosewerkzeug für Repeater-Betreiber

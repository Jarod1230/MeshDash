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
- Serial über `tokio-serial`, TCP über `tokio::net`
- [x] Mock-Transport, der Frames aus einem Skript liefert — **gehört in diesen
      Schritt, nicht später**; inklusive nachgestellter Verbindungsabbrüche
- Reconnect mit Backoff; ein abgezogenes USB-Kabel darf den Dienst nicht beenden
- `Link`-Aktor: Kommando-Warteschlange, Antwortkorrelation, Push-Verteilung

## Schritt 4 — Kern (`meshdash-core`)

- Konfiguration aus TOML und Umgebungsvariablen
- SQLite-Anbindung, Migrationsablauf über Modulgrenzen hinweg
- Event-Bus
- `Module`-Trait und Registry — der Vertrag aus [`module-system.md`](module-system.md)

## Schritt 5 — Server (`meshdash-server`)

- Axum-Router, aus der Modul-Registry zusammengebaut
- WebSocket für Live-Ereignisse
- Optionale Authentifizierung — **braucht vorher einen ADR**
- Eingebettetes Frontend, geordnetes Herunterfahren

## Schritt 6 — Erste Module

In dieser Reihenfolge, jedes für sich abgeschlossen:

1. **`system`** — Verbindungsstatus und Node-Identität. Der kleinste sinnvolle
   Durchstich vom Node bis in den Browser.
2. **`nodes`** — Kontakte und Nachbarn mit Verlauf.
3. **`messages`** — Direktnachrichten und Kanäle.
4. **`telemetry`** — Batterie und Empfangsqualität über die Zeit.

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
- **Aufbewahrung und Verdichtung von Telemetrie**
- Docker-Image, Release-Automatisierung, ARM-Builds für den Raspberry Pi

## Gesammelte Einfälle

Was auffällt, aber nicht dran ist. Landet hier statt als `TODO` im Code.

- Import bestehender Verläufe aus anderen MeshCore-Clients
- Export als CSV oder für Grafana/Prometheus
- Pfadwechsel über die Zeit sichtbar machen — vermutlich das nützlichste
  Diagnosewerkzeug für Repeater-Betreiber

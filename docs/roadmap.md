# Roadmap

Reihenfolge der Umsetzung. Kein Terminplan — eine Abhängigkeitskette.

Grundsatz: **Von unten nach oben.** Erst das Protokoll, dann der Transport, dann
der Kern, dann Module. Umgekehrt baut man eine Oberfläche für Daten, die man
noch nicht zuverlässig lesen kann.

## Schritt 1 — Gerüst

Ziel: `cargo build` und `pnpm build` laufen durch, auch wenn sie noch nichts tun.

- Cargo-Workspace mit den Crates aus [`architecture.md`](architecture.md)
- `rust-toolchain.toml`, `rustfmt.toml`
- Frontend-Gerüst: Vite, TypeScript, Tailwind, Linting
- CI: Format, Clippy, Tests, Frontend-Build
- `justfile` oder `Makefile` für die gängigen Abläufe

*Erst danach ist das Repository „grün" und alles Weitere hat ein Netz.*

## Schritt 2 — Protokoll (`meshdash-proto`)

Die fehleranfälligste Schicht, deshalb früh und mit Tests.

- Frame-Codec: Serial-Framing kodieren und dekodieren, inklusive Teil-Frames
- Opcode-Tabellen mit `Unknown(u8)`-Fallback und **Quellenangabe je Wert**
- Kodierung der Kommandos, Dekodierung der Antworten und Pushes, die belegt sind
- Unit-Tests gegen feste Byte-Arrays; Round-Trip-Tests
- Offene Punkte aus [`research/meshcore-companion-protocol.md`](research/meshcore-companion-protocol.md)
  abarbeiten oder ausdrücklich als offen markieren

**Vorbedingung:** Das Framing muss vorher an echter Hardware oder am
Firmware-Quellcode verifiziert sein — die veröffentlichte Dokumentation ist an
dieser Stelle widersprüchlich, siehe [`lessons-learned.md`](lessons-learned.md).

## Schritt 3 — Transport und Link

- `Transport`-Trait
- Serial über `tokio-serial`, TCP über `tokio::net`
- Mock-Transport, der Frames aus einem Skript liefert — **gehört in diesen
  Schritt, nicht später**
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

# MeshDash

Dashboard und Administrationsoberfläche für [MeshCore](https://meshcore.co.uk/)-Meshes.

MeshDash verbindet sich mit einem MeshCore-Companion-Node und macht daraus eine
Web-App: Nodes und Nachbarn im Blick, Nachrichtenverlauf, Telemetrie über die Zeit,
Karte, und perspektivisch Fernadministration von Repeatern und Room-Servern.

> **Status: Gerüst steht, Implementierung beginnt.**
> Cargo-Workspace, Frontend und CI bauen grün durch — aber **noch ohne
> Funktionalität**: Es gibt keinen Protokoll-Codec, keine Datenbank und keine
> API. Als Nächstes ist Schritt 2 in [`docs/roadmap.md`](docs/roadmap.md) dran.

---

## Zielbild

| Aspekt | Entscheidung |
| --- | --- |
| Backend | Rust — [ADR-0001](docs/decisions/0001-tech-stack.md) |
| Frontend | React + Vite + TypeScript — [ADR-0001](docs/decisions/0001-tech-stack.md) |
| Auslieferung | Ein einzelnes Binary mit eingebettetem Frontend |
| Architektur | Modularer Kern, Features als eigenständige Module — [ADR-0002](docs/decisions/0002-modulare-architektur.md) |
| Anbindung | USB/Serial und TCP zuerst, BLE später — [ADR-0003](docs/decisions/0003-transport-priorisierung.md) |
| Zielhardware | Läuft auf einem Raspberry Pi, nicht nur auf einem Server |

Ausführlich: [`docs/architecture.md`](docs/architecture.md).

## Warum überhaupt

Ein MeshCore-Companion-Node spricht ein binäres Protokoll über Serial, TCP oder BLE.
Die vorhandenen Clients sind auf *Bedienung* ausgelegt — ein Chat-Fenster, eine
Kontaktliste. Was fehlt, ist die *Betreiber*-Sicht: Was macht mein Mesh über die Zeit?
Welcher Repeater ist weggebrochen? Wie entwickelt sich die Batterie an Standort X?
Welche Pfade nutzt das Netz gerade? Genau da setzt MeshDash an.

## Dokumentation

Der Einstieg ist [`docs/README.md`](docs/README.md) — dort ist der gesamte
Dokumentationsbestand indiziert. Die wichtigsten Startpunkte:

- [`docs/architecture.md`](docs/architecture.md) — Zielarchitektur und Datenfluss
- [`docs/module-system.md`](docs/module-system.md) — wie Module aufgebaut sind
- [`docs/development.md`](docs/development.md) — Arbeitsumgebung einrichten
- [`docs/conventions.md`](docs/conventions.md) — Code-, Commit- und Branch-Konventionen
- [`docs/decisions/`](docs/decisions/) — Architecture Decision Records
- [`docs/lessons-learned.md`](docs/lessons-learned.md) — was uns schon auf die Füße gefallen ist
- [`docs/glossary.md`](docs/glossary.md) — MeshCore-Begriffe

Für KI-Agenten: [`CLAUDE.md`](CLAUDE.md).

## Schnellstart

```bash
git clone https://github.com/Jarod1230/MeshDash.git
cd MeshDash
just setup     # Frontend-Abhängigkeiten
just check     # alles, was auch die CI prüft
```

Ohne [`just`](https://github.com/casey/just) stehen dieselben Befehle im
[`justfile`](justfile) und in [`docs/development.md`](docs/development.md).

## Mitmachen

Siehe [`CONTRIBUTING.md`](CONTRIBUTING.md). Sicherheitsrelevantes bitte nicht
über öffentliche Issues melden — [`SECURITY.md`](SECURITY.md).

## Lizenz

[GPL-3.0-or-later](LICENSE).

MeshDash ist ein unabhängiges Projekt und steht in keiner Verbindung zum
MeshCore-Projekt oder dessen Entwicklern.

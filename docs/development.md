# Entwicklung

## Voraussetzungen

| Werkzeug | Version | Wofür |
| --- | --- | --- |
| Rust | stable, ≥ 1.85 | Backend |
| Node.js | 22 LTS | Frontend |
| pnpm | 9 oder neuer | Paketverwaltung Frontend |
| Docker | optional | Auslieferung, Testumgebung |

Rust am besten über [rustup](https://rustup.rs/). Die Toolchain ist über
`rust-toolchain.toml` festgeschrieben — ein `cargo build` im Repository genügt,
rustup holt den Rest.

### Zugriff auf die serielle Schnittstelle (Linux)

Ein Companion-Node am USB-Port erscheint als `/dev/ttyUSB0` oder `/dev/ttyACM0`.
Der Zugriff braucht Gruppenmitgliedschaft:

```bash
sudo usermod -a -G dialout "$USER"   # bei Arch/Fedora: uucp statt dialout
```

Neu anmelden, sonst greift die Gruppe nicht. **Ohne Node lässt sich trotzdem
entwickeln** — dafür gibt es den Mock-Transport, siehe [`testing.md`](testing.md).

## Repository einrichten

```bash
git clone https://github.com/Jarod1230/MeshDash.git
cd MeshDash
just setup     # entspricht: cd web && pnpm install
just check     # alles, was auch die CI prüft
```

[`just`](https://github.com/casey/just) ist optional (`cargo install just`);
alle Rezepte stehen im Klartext im [`justfile`](../justfile).

**Was schon geht:** Der Workspace baut, das Frontend baut, die CI ist grün.
**Was noch nicht geht:** alles Fachliche — der Server gibt beim Start nur seine
Version aus, die Modul-Registry ist leer. Siehe [`roadmap.md`](roadmap.md).

## Arbeitsabläufe

```bash
# Backend starten (gibt derzeit nur die Version aus)
just dev-server        # cargo run -p meshdash-server

# Frontend im Entwicklungsmodus, Proxy /api auf das Backend
just dev-web           # cd web && pnpm dev

# Prüfungen — identisch mit der CI
just check
just check-rust        # fmt, clippy, cargo test
just check-web         # lint, typecheck, test, build
just check-docs        # interne Markdown-Links
```

Im Entwicklungsmodus laufen zwei Prozesse: Vite liefert das Frontend mit Hot
Reload und leitet `/api` an das Rust-Backend weiter. Im Release-Build wird das
gebaute Frontend ins Binary eingebettet — dann ist es ein Prozess.

## Konfiguration

MeshDash liest `meshdash.toml` aus dem Arbeitsverzeichnis; Umgebungsvariablen
mit dem Präfix `MESHDASH_` überschreiben einzelne Werte. Die geplanten Optionen
stehen in [`configuration.md`](configuration.md).

`meshdash.toml` ist in `.gitignore` — die lokale Konfiguration enthält
Geräte-Pfade und möglicherweise Zugangsdaten und gehört nicht ins Repository.

## Fehlersuche

- **Logs:** über `RUST_LOG` steuerbar, z. B. `RUST_LOG=meshdash=debug`.
- **Rohe Frames:** Wenn das Protokoll nicht tut, was es soll, ist der Hexdump
  der Frames der schnellste Weg. Diese Ebene ist von Anfang an als eigenes
  Log-Target vorgesehen.
- **Wenn ein Wert nicht stimmt:** nicht raten. Der Stand der Protokollrecherche
  liegt in [`research/meshcore-companion-protocol.md`](research/meshcore-companion-protocol.md),
  inklusive der offenen Fragen.

## Häufige Stolpersteine

Gesammelt in [`lessons-learned.md`](lessons-learned.md). Vor dem ersten
Protokoll-Debugging lohnt sich ein Blick — dort steht unter anderem, warum die
Upstream-Dokumentation zum Framing widersprüchlich ist.

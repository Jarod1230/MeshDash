# Entwicklung

## Voraussetzungen

| Werkzeug | Version | Wofür |
| --- | --- | --- |
| Rust | stable, ≥ 1.85 | Backend |
| Node.js | 22 LTS | Frontend |
| pnpm | 9 oder neuer | Paketverwaltung Frontend |
| Docker | optional | Auslieferung, Testumgebung |

Rust am besten über [rustup](https://rustup.rs/). Die Toolchain wird über
`rust-toolchain.toml` festgeschrieben, sobald der Workspace existiert — dann
genügt ein `cargo build` im Repository, rustup holt den Rest.

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
```

Mehr ist derzeit nicht zu tun: Das Repository enthält noch keinen Code.
Was der erste Implementierungsschritt anlegt, steht in [`roadmap.md`](roadmap.md).

## Arbeitsabläufe (sobald der Code existiert)

Die folgenden Befehle sind das Ziel. Sie funktionieren, sobald Schritt 1 der
Roadmap umgesetzt ist.

```bash
# Backend bauen und starten
cargo run -p meshdash-server

# Frontend im Entwicklungsmodus (Proxy auf das Backend)
cd web && pnpm install && pnpm dev

# Prüfungen — dieselben, die die CI fahren wird
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cd web && pnpm lint && pnpm typecheck && pnpm test
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

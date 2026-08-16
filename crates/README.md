# crates/

Rust-Workspace. Gebaut wird von unten nach oben — bislang ist nur die unterste
Schicht angefangen; die übrigen `lib.rs` enthalten weiterhin nur den
Modul-Kommentar mit den Regeln für ihre Schicht.

Zuschnitt, begründet in [`../docs/architecture.md`](../docs/architecture.md):

| Crate | Verantwortung | Darf abhängen von | Stand |
| --- | --- | --- | --- |
| `meshdash-proto` | Companion-Protokoll: Framing, Opcodes, Kodierung. **Keine I/O.** | — | Frame-Ebene fertig, Opcodes offen |
| `meshdash-transport` | Serial, TCP, Mock, später BLE. Verbindung und Reconnect. **Kein Protokollwissen.** | `proto` | Trait und Mock fertig, Serial und TCP offen |
| `meshdash-core` | Konfiguration, SQLite, Event-Bus, Modul-Registry. **Keine Fachlichkeit.** | `proto`, `transport` | leer |
| `meshdash-modules` | Alle fachlichen Module. | `core` | leer |
| `meshdash-server` | Axum, WebSocket, Auth, eingebettetes Frontend. | alle | gibt die Version aus |

In `meshdash-proto` liegt das Modul `frame`: `encode()` für ausgehende Frames
und `Decoder` für eingehende, der Teil-Frames puffert und nach Störungen wieder
aufsynchronisiert. Das Wire-Format ist am Firmware-Quellcode verifiziert, jeder
Wert nennt seine Quelle im Doc-Kommentar. Die **Opcodes darüber sind es nicht** —
für sie gilt weiterhin: nicht raten, siehe
[`../docs/research/meshcore-companion-protocol.md`](../docs/research/meshcore-companion-protocol.md).

Die Abhängigkeitsrichtung ist strikt von oben nach unten. Ein Rückwärtsbezug —
etwa `core`, das ein Modul kennt — ist ein Architekturfehler, kein
Sonderfall. Siehe [`../docs/module-system.md`](../docs/module-system.md).

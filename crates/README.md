# crates/

Rust-Workspace. Die Crates existieren und bauen, sind aber **inhaltlich leer** —
jede `lib.rs` enthält nur den Modul-Kommentar mit den Regeln für diese Schicht.
Gefüllt werden sie ab Schritt 2 der [Roadmap](../docs/roadmap.md).

Zuschnitt, begründet in [`../docs/architecture.md`](../docs/architecture.md):

| Crate | Verantwortung | Darf abhängen von |
| --- | --- | --- |
| `meshdash-proto` | Companion-Protokoll: Framing, Opcodes, Kodierung. **Keine I/O.** | — |
| `meshdash-transport` | Serial, TCP, Mock, später BLE. Verbindung und Reconnect. **Kein Protokollwissen.** | `proto` |
| `meshdash-core` | Konfiguration, SQLite, Event-Bus, Modul-Registry. **Keine Fachlichkeit.** | `proto`, `transport` |
| `meshdash-modules` | Alle fachlichen Module. | `core` |
| `meshdash-server` | Axum, WebSocket, Auth, eingebettetes Frontend. | alle |

Die Abhängigkeitsrichtung ist strikt von oben nach unten. Ein Rückwärtsbezug —
etwa `core`, das ein Modul kennt — ist ein Architekturfehler, kein
Sonderfall. Siehe [`../docs/module-system.md`](../docs/module-system.md).

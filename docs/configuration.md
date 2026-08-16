# Konfiguration

> **Geplant.** MeshDash liest noch keine Konfiguration — der Kern existiert
> nicht. Dieses Dokument beschreibt die vorgesehene Oberfläche und ist beim
> Umsetzen von Schritt 4 der [Roadmap](roadmap.md) auf den tatsächlichen Stand
> zu bringen.

## Quellen und Rangfolge

Später gewinnt:

1. Voreinstellungen im Code
2. `meshdash.toml` im Arbeitsverzeichnis (Pfad über `--config` überschreibbar)
3. Umgebungsvariablen mit Präfix `MESHDASH_`
4. Kommandozeilenargumente

Umgebungsvariablen bilden die Verschachtelung mit doppeltem Unterstrich ab:
`[server] port` wird zu `MESHDASH_SERVER__PORT`.

`meshdash.toml` steht in `.gitignore`. Geräte-Pfade und Zugangsdaten gehören
nicht ins Repository.

## Vorgesehene Optionen

```toml
[server]
# Adresse, auf der die Weboberfläche lauscht.
# Standard ist localhost — MeshDash gehört hinter einen Reverse-Proxy,
# nicht ungeschützt ins Netz. Siehe SECURITY.md.
bind = "127.0.0.1:8080"

[auth]
# Optionales Bearer-Token. Nicht gesetzt = keine Authentifizierung.
# Die endgültige Form ist noch nicht entschieden und braucht einen ADR.
token = ""

[database]
# Pfad zur SQLite-Datei. Wird beim Start angelegt.
path = "data/meshdash.db"

[node]
# Anbindung an den Companion-Node: "serial", "tcp" oder "mock".
transport = "serial"

[node.serial]
port = "/dev/ttyUSB0"
baud = 115200

[node.tcp]
host = "192.168.1.50"
port = 5000

[node.mock]
# Skriptdatei mit Frames für Entwicklung ohne Hardware. Siehe testing.md.
script = "fixtures/basic.jsonl"

[log]
# Entspricht RUST_LOG. Die Umgebungsvariable gewinnt.
filter = "meshdash=info"

# Module bringen ihre eigene Konfiguration unter [modules.<name>] mit.
# Ein nicht aufgeführtes Modul läuft mit seinen Voreinstellungen.
[modules.telemetry]
enabled = true

[modules.messages]
enabled = true
```

## Beim Ergänzen einer Option

1. Option in dieses Dokument aufnehmen, mit Kommentar wozu sie dient.
2. Voreinstellung so wählen, dass MeshDash **ohne** Konfigurationsdatei startet.
3. Modul-Optionen gehören unter `[modules.<name>]` und werden vom Modul gelesen,
   nicht vom Kern.
4. Sicherheitsrelevante Voreinstellungen restriktiv wählen. `bind` steht
   bewusst auf `127.0.0.1`.

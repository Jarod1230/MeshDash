# Konfiguration

> **Umgesetzt.** Die unten beschriebenen Optionen werden gelesen **und wirken**:
> Der Server lauscht auf `[server] bind`, die Datenbank wird unter
> `[database] path` angelegt, `[node]` bestimmt die Anbindung an den
> Companion-Node, und `[auth]` schützt die API. Was noch fehlt, steht am Ende
> des Dokuments.

## Quellen und Rangfolge

Später gewinnt:

1. Voreinstellungen im Code
2. `meshdash.toml` im Arbeitsverzeichnis (Pfad über `--config` überschreibbar)
3. Umgebungsvariablen mit Präfix `MESHDASH_`
4. Kommandozeilenargumente

Umgesetzt sind 1 bis 3. Kommandozeilenargumente gehören zum Binary und kommen
mit Schritt 5.

**Eine unbekannte Option ist ein Fehler**, kein stillschweigend ignorierter
Eintrag. Ein verschriebenes `bnid` statt `bind` würde sonst dazu führen, dass
der Dienst auf einer anderen Adresse lauscht als beabsichtigt — und niemand
bekäme es mit.

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
# Bearer-Token. Ist es gesetzt, braucht jede Anfrage unter /api/v1/ den Header
# "Authorization: Bearer <token>". Nicht gesetzt = keine Authentifizierung.
# Siehe ADR-0006.
token = ""

# Erlaubt das Lauschen auf einer öffentlichen Adresse ohne Token — für den
# Betrieb hinter einem Reverse-Proxy, der die Authentifizierung übernimmt.
# Ohne diese Zustimmung startet MeshDash in dieser Kombination nicht.
allow_unauthenticated = false

[database]
# Pfad zur SQLite-Datei. Wird beim Start angelegt.
path = "data/meshdash.db"

[node]
# Anbindung an den Companion-Node: "serial" oder "tcp".
transport = "serial"

[node.serial]
port = "/dev/ttyUSB0"
# Am Firmware-Quellcode belegt, siehe research/meshcore-companion-protocol.md.
baud = 115200

[node.tcp]
host = "127.0.0.1"
port = 5000

[log]
# Entspricht RUST_LOG. Die Umgebungsvariable gewinnt.
filter = "meshdash=info"
```

## Was noch fehlt

Bewusst noch nicht umgesetzt, damit hier nichts steht, was nicht funktioniert:

- **`[modules.<name>]`** — es gibt noch keine Module. Die Sektion kommt mit
  Schritt 6; gelesen wird sie dann vom Modul, nicht vom Kern.
- **`[node.mock]` mit Skriptdatei** — der Mock-Transport spielt Skripte ab, die
  im Code zusammengesetzt werden, und lädt keine Dateien. Eine Option dafür
  anzubieten, würde eine Fähigkeit vortäuschen. Sie ergibt Sinn, sobald es
  aufgezeichneten Verkehr unter `fixtures/` gibt — siehe
  [`testing.md`](testing.md).
- **Kommandozeilenargumente**, insbesondere `--config`. Gehören zum Binary.
- **Authentifizierung für den WebSocket.** Browser können bei
  WebSocket-Verbindungen keinen eigenen `Authorization`-Header setzen; dafür
  braucht es einen eigenen Weg, siehe
  [ADR-0006](decisions/0006-authentifizierung.md).

## Beim Ergänzen einer Option

1. Option in dieses Dokument aufnehmen, mit Kommentar wozu sie dient.
2. Voreinstellung so wählen, dass MeshDash **ohne** Konfigurationsdatei startet.
3. Modul-Optionen gehören unter `[modules.<name>]` und werden vom Modul gelesen,
   nicht vom Kern.
4. Sicherheitsrelevante Voreinstellungen restriktiv wählen. `bind` steht
   bewusst auf `127.0.0.1`.

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

# Am Ereignisstrom /api/v1/events wird dasselbe Token verlangt, aber anders
# übergeben: Ein Browser kann bei WebSocket-Verbindungen keinen eigenen Header
# setzen, deshalb ist die **erste Nachricht** nach dem Verbindungsaufbau das
# Token. Ein Token im Query-String wäre einfacher, würde aber in Server- und
# Proxy-Protokollen sowie im Verlauf des Browsers landen.

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

# Abschnitte unter [modules.<name>] gehören dem jeweiligen Modul. Der Kern
# trägt sie nur weiter und liest sie nicht; was eine Option bedeutet, steht
# beim Modul. Siehe module-system.md.
[modules.telemetry]
# Andere Knoten nach ihren Messwerten fragen. Standardmäßig aus: Jede Anfrage
# geht über Funk und belegt Sendezeit im Band, das sich das ganze Mesh teilt.
# Das ist eine Entscheidung des Betreibers, keine Voreinstellung.
neighbours = false
# Minuten zwischen zwei Anfragen. Es wird immer nur *ein* Knoten pro Runde
# gefragt, reihum.
every_minutes = 30
# Knoten, die so lange nichts von sich hören ließen, werden übersprungen —
# an etwas zu senden, das nicht da ist, kostet nur Sendezeit.
silent_after_hours = 24

[modules.tiles]
# Woher die Kartenkacheln kommen, als Vorlage mit {z}, {x} und {y}.
# Leer heißt: keine Kacheln. Das ist der Auslieferungszustand — MeshDash läuft
# an Orten ohne Uplink, und einen öffentlichen Server für den Betreiber
# auszuwählen würde dort scheitern und nebenbei einem Fremden verraten, wo das
# Mesh steht. Siehe ADR-0011.
source = ""
# Wem die Karte gehört. **Pflicht, sobald `source` gesetzt ist** — jeder
# brauchbare Kacheldienst verlangt die Nennung in seinen Bedingungen, und der
# Dienst startet ohne sie nicht. Erscheint auf der Karte.
attribution = ""
# Wohin geholte Kacheln gelegt werden. Ein Dateibaum <z>/<x>/<y>.<endung>.
cache_dir = "data/tiles"
# Tiefster Zoom, der weitergereicht wird. Tiefer zu fragen als die Quelle hat,
# bringt nur 404er — auf deren Kosten.
max_zoom = 19
# Womit MeshDash sich beim Kacheldienst meldet. Nennt das Projekt, nicht den
# Betreiber. Viele Dienste sperren allgemeine Kennungen.
user_agent = "MeshDash/<version> (+https://github.com/Jarod1230/MeshDash)"
# Wie viele Abrufe gleichzeitig hinausgehen dürfen. Eine gezogene Karte fragt
# schneller nach Kacheln, als eine Quelle beantworten möchte; der Rest wartet.
max_concurrent_fetches = 4

[modules.traffic]
# Den Paketverlauf mitschreiben. Der Node meldet jedes gehörte Paket von
# selbst; ob MeshDash es behält, ist diese Entscheidung. Aus heißt: Die
# Verdichtung „wer hört wen" entsteht weiter, der Verlauf nicht.
record = true
# Wie viele Tage Paketverlauf aufbewahrt werden. Großzügig, weil MeshDash ein
# Analysewerkzeug ist — wer eine Störung von vorletzter Woche nachvollziehen
# will, braucht die Pakete und nicht ihre Zusammenfassung. Siehe ADR-0016.
# Die Verdichtung unterliegt keiner Frist; sie wächst mit dem Mesh, nicht mit
# dem Verkehr.
keep_days = 30
```

## Was noch fehlt

Bewusst noch nicht umgesetzt, damit hier nichts steht, was nicht funktioniert:

- **`[node.mock]` mit Skriptdatei** — der Mock-Transport spielt Skripte ab, die
  im Code zusammengesetzt werden, und lädt keine Dateien. Eine Option dafür
  anzubieten, würde eine Fähigkeit vortäuschen. Sie ergibt Sinn, sobald es
  aufgezeichneten Verkehr unter `fixtures/` gibt — siehe
  [`testing.md`](testing.md).
- **Kommandozeilenargumente**, insbesondere `--config`. Gehören zum Binary.

## Was die Oberfläche ändern kann, und was nicht

Seit [ADR-0017](decisions/0017-einstellungen-zur-laufzeit.md) sind
**Modul-Optionen im Betrieb änderbar** — unter „Einstellungen" in der
Oberfläche. Was dort geändert wird, liegt in der Datenbank und gewinnt gegen
die Datei, Option für Option. Alles Unberührte kommt weiter aus der Datei; die
Seite zeigt an, wo etwas abweicht.

**Nicht änderbar sind die Optionen, die entscheiden, wie der Dienst startet:**
`[server]`, `[auth]`, `[database]`, `[node]`, `[log]` — und `[modules.tiles]`,
weil der Kachelclient beim Start gebaut wird. Sie gelten ab dem nächsten Start.

Geändert wird nie die Datei selbst. Sie gehört dem Betreiber, samt ihren
Kommentaren.

## Beim Ergänzen einer Option

1. Option in dieses Dokument aufnehmen, mit Kommentar wozu sie dient.
2. Voreinstellung so wählen, dass MeshDash **ohne** Konfigurationsdatei startet.
3. Modul-Optionen gehören unter `[modules.<name>]` und werden vom Modul gelesen,
   nicht vom Kern.
4. Sicherheitsrelevante Voreinstellungen restriktiv wählen. `bind` steht
   bewusst auf `127.0.0.1`.
5. Soll sie im Betrieb änderbar sein, in `crates/meshdash-server/src/settings.rs`
   und in die Liste auf der Einstellungsseite aufnehmen — dort braucht sie einen
   Satz dazu, **was sie kostet**. Und das Modul muss sie beim Benutzen lesen,
   nicht beim Start festhalten.

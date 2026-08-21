# Modulsystem

MeshDash soll über Jahre wachsen, ohne dass der Kern mitwächst. Deshalb ist
jede Fachlichkeit ein **Modul**: eine abgeschlossene Einheit mit eigenen
Tabellen, eigenen HTTP-Routen und eigener Oberfläche.

Dieses Dokument beschreibt den Zuschnitt. Der Vertrag steht als
`module::Module` in `meshdash-core`.

## Der Vertrag

Ein Modul liefert vier Dinge:

| Bestandteil | Bedeutung |
| --- | --- |
| `name()` | Kennung des Moduls. Präfixt Tabellen (`<name>_…`), Routen (`/api/v1/<name>/…`) und die Migrationsbuchführung. **Stabil halten** — eine Umbenennung verwaist Tabellen und Schemaversion. |
| `migrations()` | Schemageschichte des Moduls, aufsteigend ab 1. Leer ist zulässig für ein Modul, das nichts speichert. |
| `routes()` | HTTP-Routen des Moduls, relativ zu seinem eigenen Präfix. `None` für ein Modul ohne API. **Pfade benennen** — eine Route auf `/` greift im eingehängten Router nicht. |
| `start(ctx)` | Wird einmal aufgerufen, **nachdem** die Migrationen liefen. Startet Event-Handler und Hintergrundjobs. Dauerhaftes läuft in einem eigenen Task; die Rückkehr heißt „läuft", nicht „fertig". |

Dazu bekommt es einen `AppContext` mit Datenbank, Event-Bus und einem Handle
zum Node — mehr nicht. Der schmale Zuschnitt ist Absicht: Was ein Modul nicht
in die Hand bekommt, kann es auch nicht missbrauchen.

**Doppelte Modulnamen werden abgewiesen.** Der Name entscheidet über
Tabellenpräfix, Routenpfad und Migrationszuordnung; zwei Anwärter würden sich
gegenseitig das Schema migrieren.

**`events` ist reserviert.** Unter `/api/v1/events` liegt der Ereignisstrom des
Kerns; ein gleichnamiges Modul käme dort nicht an.

**Ein fehlschlagendes Modul verhindert den Start.** Mit halbem Schema
weiterzulaufen erzeugt falsche Daten statt eines Fehlers, und das ist schlimmer
als ein Dienst, der gar nicht erst hochkommt. Die Fehlermeldung nennt das
verantwortliche Modul.

**Routen liefert das Modul, eingehängt werden sie vom Server.** Ein Modul gibt
Pfade relativ zu sich selbst an — `/status`, nicht `/api/v1/system/status`. Wo
sie landen, entscheidet `meshdash-server`; so steht die Präfixkonvention an
einer Stelle statt in jedem Modul.

## Was ein Modul ist

Ein Modul beantwortet genau eine fachliche Frage des Betreibers. „Welche Nodes
kenne ich?" ist ein Modul. „Wie war die Batterie letzte Woche?" ist ein Modul.
„Eine Tabellenkomponente" ist keins.

Ein Modul besteht aus zwei Hälften, die denselben Namen tragen:

- **Backend** — Migrationen, Routen, Event-Handler, optional Hintergrundjobs.
- **Frontend** — Seiten, Navigationseinträge, Widgets fürs Dashboard.

Beide melden sich bei einer Registry an. Ein Modul zu entfernen bedeutet, es aus
zwei Registrierungslisten zu streichen — nicht, Code aus dem Kern zu schneiden.
Das ist der Lackmustest für den Schnitt.

## Was ein Modul darf und was nicht

**Darf:**

- eigene Tabellen anlegen und besitzen (Präfix `<modul>_`)
- eigene Routen unter `/api/v1/<modul>/…` anbieten
- auf dem Event-Bus mithören und selbst Ereignisse veröffentlichen
- Kommandos über den `Link` an den Node schicken
- eigene Konfiguration unter `[modules.<modul>]` lesen —
  `context.settings.get::<MeineOptionen>("modul")`. Der Kern trägt den
  Abschnitt nur weiter; was darin steht, weiß nur das Modul. Fehlt der
  Abschnitt, gilt der Standardwert des Typs, damit ein Modul auch
  unkonfiguriert läuft. Ein Abschnitt, der **nicht passt**, ist ein Fehler und
  kein stiller Rückfall — eine verschriebene Option, die wirkungslos bleibt,
  fällt sonst niemandem auf.

**Darf nicht:**

- in die Tabellen eines anderen Moduls schreiben oder lesen
- ein anderes Modul direkt aufrufen (Kopplung läuft über den Event-Bus)
- Code in `meshdash-core` ergänzen, um sich selbst zu ermöglichen
- voraussetzen, dass ein anderes Modul aktiv ist

Die letzte Regel ist die wichtigste: Module müssen einzeln abschaltbar bleiben.
Wenn `telemetry` ohne `nodes` nicht startet, ist der Schnitt falsch.

## Kopplung über den Event-Bus

Module hängen nicht aneinander, sondern am selben Bus. Der Bus verteilt
Ereignisse per Broadcast; jedes Modul entscheidet selbst, was es interessiert.

```
                       ┌─────────────┐
   Link (vom Node) ───►│  Event-Bus  │───► nodes     (schreibt Kontakte fort)
                       │  Broadcast  │───► telemetry (schreibt Messwerte fort)
   Module ────────────►│             │───► WebSocket (schiebt an den Browser)
                       └─────────────┘
```

**Pushes werden nicht selbst zerlegt.** Was ein `AppEvent::Push` bedeutet,
beantwortet `meshdash_proto::push::PushEvent::parse` — ein `match` darüber, statt
Opcodes im Modul zu prüfen. Sonst verteilt sich Protokollwissen über Module, die
es nichts angeht, und jede neue Firmware müsste an mehreren Stellen nachgezogen
werden.

Braucht ein Modul Daten eines anderen, gibt es zwei zulässige Wege: das
besitzende Modul veröffentlicht sie als Ereignis, oder es bietet eine
ausdrückliche Schnittstelle an. Ein `JOIN` über Modulgrenzen ist keiner davon.

**Was über den Bus geht, wird mit beiden Modulen getestet.** Die Testdatei
`crates/meshdash-modules/tests/module_coupling.rs` registriert die beteiligten
Module gemeinsam. Der Grund steht in
[`lessons-learned.md`](lessons-learned.md): Eine Kopplung, von der nur die
empfangende Hälfte getestet ist, kann vollständig fehlen, während beide
Testsuiten grün sind.

Für den ersten Weg gibt es `AppEvent::Module`:

```rust
context.events.publish(AppEvent::Module {
    module: "messages".into(),   // wer veröffentlicht
    kind: "signal".into(),       // was, im Vokabular dieses Moduls
    data: serde_json::json!({ "snr": -2.5, "path_len": 2 }),
});
```

Der Kern trägt das nur weiter und liest es nicht — Begründung in
[ADR-0007](decisions/0007-modul-ereignisse.md). Daraus folgen drei Pflichten:

- **Wer veröffentlicht, dokumentiert die Nutzlast** in seinem eigenen Modul.
  Es gibt keine Typprüfung, die das ersetzt.
- **Wer empfängt, filtert auf `module` und `kind`** und überspringt eine
  Nutzlast, die nicht passt, statt daran zu scheitern.
- **Keine Geheimnisse in `data`.** Der Ereignisstrom unter `/api/v1/events`
  gibt diese Ereignisse an den Browser weiter.

Umgesetzt zwischen `messages` und `telemetry`: Der SNR kommt mit jeder
Nachricht herein, gehört aber in die Zeitreihe des Telemetriemoduls.

## Geplante Module

Reihenfolge und Zuschnitt aus [`roadmap.md`](roadmap.md). Die Tabelle ist zu
pflegen, sobald sich etwas ändert.

| Modul | Zweck | Stand |
| --- | --- | --- |
| `system` | Verbindungsstatus, Node-Identität, Version, Health | **umgesetzt** — `/api/v1/system/{status,connections}`, mit Oberfläche |
| `nodes` | Kontakte und Nachbarn, Erstsichtung, Letztsichtung, Pfade, Position | **umgesetzt** — `/api/v1/nodes/{contacts,adverts}`, mit Liste, Netz- und Kartenansicht |
| `messages` | Direktnachrichten und Kanäle, Verlauf, Senden | **umgesetzt** — Gespräche unter `/api/v1/messages/{conversations,conversation}`, dazu die flachen Listen und das Senden |
| `telemetry` | Batterie, SNR/RSSI und weitere Messwerte über die Zeit | **umgesetzt** — `/api/v1/telemetry/{battery,signal,neighbours}`; Nachbarabfrage abschaltbar |
| `admin` | Fernadministration von Repeatern und Room-Servern | angedacht |
| `alerts` | Benachrichtigung, wenn ein Node ausfällt | angedacht |

## Wann eine Darstellung kein eigenes Modul ist

Die Karte war als Modul `map` geplant und ist keines geworden. Der Grund steht
in [ADR-0010](decisions/0010-karte.md) und ist verallgemeinerbar: Ein Modul
beantwortet **eine fachliche Frage**. „Wo sind meine Knoten" ist keine andere
Frage als „welche Knoten habe ich", sondern dieselbe in anderer Darstellung —
und die Positionen gehören bereits `nodes`.

Ein eigenes Modul hätte sich denselben Bestand über den Ereignisbus noch einmal
aufbauen müssen, nur um die Regel „keine fremden Tabellen" einzuhalten. Die
Regel schützt vor **Kopplung**, nicht vor Darstellung; sie so zu befolgen hätte
eine dritte Kopie derselben Daten erzeugt, die auseinanderlaufen kann.

Faustregel: Eine neue **Sicht** auf vorhandene Daten gehört in das Modul, dem
die Daten gehören. Ein neues Modul entsteht, wenn eine Frage hinzukommt, die
eigene Daten braucht.

## Ein Modul anlegen

Der genaue Ablauf steht fest, sobald der Kern existiert. Der Rahmen:

1. **Zuschnitt klären** — Issue mit der Vorlage *Modul-Vorschlag*. Welche Frage
   beantwortet das Modul? Welche Tabellen besitzt es? Welche Ereignisse
   verarbeitet es, welche veröffentlicht es?
2. **Backend** — Verzeichnis unter `crates/meshdash-modules/src/<modul>/`,
   `Module`-Trait implementieren, Migrationen unter `migrations/` ablegen,
   in der Registry eintragen.
3. **Frontend** — Verzeichnis unter `web/src/modules/<modul>/`, ein
   `UiModule` exportieren (`id`, `title`, `summary`, `path`, `component`) und
   in `web/src/modules/index.ts` eintragen. Die Navigation entsteht aus dieser
   Liste; `App.tsx` wird dafür **nicht** angefasst. Bausteine für Signal,
   Zeitangaben, Karten, Diagramme und Zustände liegen unter `web/src/ui/`.
   Für Live-Aktualisierung sagt die Seite mit `useLiveReload`, auf welche
   Ereignisse sie reagieren will — sie baut ihren Zustand **nicht** aus dem
   Ereignisstrom nach, sondern lädt neu.
4. **Tests** — Routen gegen einen Mock-Link, Event-Handler gegen synthetische
   Ereignisse.
5. **Dokumentation** — Zeile in der Tabelle oben, Konfigurationsoptionen nach
   [`configuration.md`](configuration.md), nutzersichtbare Änderung ins
   `CHANGELOG.md`.

## Wann etwas *kein* Modul ist

Nicht alles wird ein Modul, sonst bekommt der Kern zwanzig Registrierungen für
Kleinkram. In den Kern gehört, was **jedes** Modul braucht: Konfiguration,
Datenbankzugriff, Event-Bus, Transport, Authentifizierung, HTTP-Grundgerüst.

Faustregel: Wenn du beim Entfernen des Codes mehr als eine Registrierungsliste
anfassen müsstest, ist es Kern. Wenn du dabei anderen Modulen wehtust, ist der
Schnitt falsch.

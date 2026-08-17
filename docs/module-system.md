# Modulsystem

MeshDash soll über Jahre wachsen, ohne dass der Kern mitwächst. Deshalb ist
jede Fachlichkeit ein **Modul**: eine abgeschlossene Einheit mit eigenen
Tabellen, eigenen HTTP-Routen und eigener Oberfläche.

Dieses Dokument beschreibt den Zuschnitt. Der Vertrag steht als
`module::Module` in `meshdash-core`.

## Der Vertrag

Ein Modul liefert drei Dinge:

| Bestandteil | Bedeutung |
| --- | --- |
| `name()` | Kennung des Moduls. Präfixt Tabellen (`<name>_…`), Routen (`/api/v1/<name>/…`) und die Migrationsbuchführung. **Stabil halten** — eine Umbenennung verwaist Tabellen und Schemaversion. |
| `migrations()` | Schemageschichte des Moduls, aufsteigend ab 1. Leer ist zulässig für ein Modul, das nichts speichert. |
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

**Routen sind noch nicht Teil des Traits.** Sie gehören dazu, aber es gibt bis
Schritt 5 der [Roadmap](roadmap.md) keinen Router, an dem sie hängen könnten —
eine Schnittstelle zu entwerfen, die nichts ausüben kann, wäre geraten statt
entschieden.

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
- eigene Konfiguration unter `[modules.<modul>]` lesen

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

Braucht ein Modul Daten eines anderen, gibt es zwei zulässige Wege: das
besitzende Modul veröffentlicht sie als Ereignis, oder es bietet eine
ausdrückliche Schnittstelle an. Ein `JOIN` über Modulgrenzen ist keiner davon.

## Geplante Module

Reihenfolge und Zuschnitt aus [`roadmap.md`](roadmap.md). Die Tabelle ist zu
pflegen, sobald sich etwas ändert.

| Modul | Zweck | Stand |
| --- | --- | --- |
| `system` | Verbindungsstatus, Node-Identität, Version, Health | **umgesetzt** — `/api/v1/system/status` |
| `nodes` | Kontakte und Nachbarn, Erstsichtung, Letztsichtung, Pfade | geplant |
| `messages` | Direktnachrichten und Kanäle, Verlauf, Senden | geplant |
| `telemetry` | Batterie, SNR/RSSI und weitere Messwerte über die Zeit | geplant |
| `map` | Geografische Darstellung bekannter Positionen | angedacht |
| `admin` | Fernadministration von Repeatern und Room-Servern | angedacht |
| `alerts` | Benachrichtigung, wenn ein Node ausfällt | angedacht |

## Ein Modul anlegen

Der genaue Ablauf steht fest, sobald der Kern existiert. Der Rahmen:

1. **Zuschnitt klären** — Issue mit der Vorlage *Modul-Vorschlag*. Welche Frage
   beantwortet das Modul? Welche Tabellen besitzt es? Welche Ereignisse
   verarbeitet es, welche veröffentlicht es?
2. **Backend** — Verzeichnis unter `crates/meshdash-modules/src/<modul>/`,
   `Module`-Trait implementieren, Migrationen unter `migrations/` ablegen,
   in der Registry eintragen.
3. **Frontend** — Verzeichnis unter `web/src/modules/<modul>/`, Modul-Manifest
   exportieren, in der Frontend-Registry eintragen.
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

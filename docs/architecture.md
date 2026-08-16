# Architektur

Zielbild. Beschreibt, wohin gebaut wird — noch nicht, was existiert.
Der Umsetzungsstand steht in [`roadmap.md`](roadmap.md).

## Leitgedanken

1. **Ein Artefakt.** Ein Binary, das Frontend eingebettet, SQLite als Datenbank.
   Kein Container-Verbund, kein separater Datenbankserver. MeshDash soll auf
   einem Raspberry Pi neben dem Companion-Node laufen können.
2. **Der Kern kennt keine Fachlichkeit.** Der Kern kann Transport, Persistenz,
   Ereignisverteilung und HTTP. Was „ein Node", „eine Nachricht" oder „eine
   Batteriekurve" ist, weiß nur das jeweilige Modul.
3. **Der Node ist die Wahrheit, die Datenbank das Gedächtnis.** MeshDash
   erfindet keinen Mesh-Zustand. Es schreibt mit, was der Companion-Node meldet,
   und macht daraus einen Verlauf.
4. **Ohne Hardware entwickelbar.** Jede Schicht muss sich mit einem Mock testen
   lassen. Wer keinen Node am USB-Port hat, muss trotzdem am Projekt arbeiten können.

## Schichten

```
┌──────────────────────────────────────────────────────────────┐
│  Browser — React + Vite + TypeScript                         │
│  Modul-Registry: jedes Modul bringt Routen und Navigation mit │
└───────────────┬──────────────────────────┬───────────────────┘
                │ REST /api/v1             │ WebSocket /api/v1/events
┌───────────────▼──────────────────────────▼───────────────────┐
│  meshdash-server — HTTP, WebSocket, Auth, Static-Embed        │
│  baut den Router aus der Modul-Registry zusammen              │
└───────────────┬──────────────────────────────────────────────┘
                │
┌───────────────▼──────────────────────────────────────────────┐
│  meshdash-modules — nodes │ messages │ telemetry │ map │ …    │
│  je Modul: Migrationen, Routen, Event-Handler, Hintergrundjobs│
└───────────────┬──────────────────────────────────────────────┘
                │  Module-Trait, Event-Bus, AppContext
┌───────────────▼──────────────────────────────────────────────┐
│  meshdash-core — Konfiguration, SQLite, Event-Bus,            │
│                  Modul-Registry, Fehlertypen                  │
└───────────────┬──────────────────────────────────────────────┘
                │  Link: Request/Response + Push-Stream
┌───────────────▼──────────────────────────────────────────────┐
│  meshdash-transport — Serial │ TCP │ (BLE später) │ Mock      │
│  Verbindungsaufbau, Reconnect, Framing über die Leitung       │
└───────────────┬──────────────────────────────────────────────┘
                │  Frames
┌───────────────▼──────────────────────────────────────────────┐
│  meshdash-proto — Companion-Protokoll: Framing, Opcodes,      │
│                   Kodierung und Dekodierung. Keine I/O.       │
└──────────────────────────────────────────────────────────────┘
                          ↕ USB / TCP
                   MeshCore-Companion-Node
```

### Warum diese Schnitte

- **`meshdash-proto` ohne I/O.** Reine Byte-Übersetzung, synchron, ohne Tokio.
  Dadurch ist die fehleranfälligste Schicht mit gewöhnlichen Unit-Tests aus
  Byte-Arrays prüfbar — ohne Hardware, ohne Laufzeitumgebung.
- **`meshdash-transport` ohne Protokollwissen.** Kennt Leitungen und
  Wiederverbindung, aber keine Opcodes. Ein neuer Transport (BLE) berührt
  weder Protokoll noch Fachlogik.
- **`meshdash-core` ohne Fachlichkeit.** Sonst wächst der Kern mit jedem Feature,
  und genau das soll die Modularität verhindern.
- **`meshdash-modules` als Ort für alles Fachliche.** Siehe
  [`module-system.md`](module-system.md).

## Datenfluss

**Eingehend** — der Node meldet etwas von sich aus:

```
Node ──frame──> Transport ──decode──> Link ──AppEvent──> Event-Bus
                                                              │
                              ┌───────────────────────────────┤
                              ▼                               ▼
                    Modul schreibt in SQLite        WebSocket an Browser
```

Module hören auf dem Bus, entscheiden selbst, was sie interessiert, und
persistieren in ihre eigenen Tabellen. Der Bus ist Broadcast: mehrere Module
dürfen dasselbe Ereignis unabhängig verarbeiten.

**Ausgehend** — der Browser löst etwas aus:

```
Browser ──HTTP──> Modul-Route ──Command──> Link ──encode──> Transport ──> Node
                                             │
                                             └── wartet auf korrelierte Antwort
```

Der `Link` ist der Aktor, der die serielle Natur der Verbindung kapselt: Ein
Companion-Node beantwortet Kommandos der Reihe nach. Der Link nimmt Kommandos
entgegen, ordnet Antworten den Anfragen zu und verteilt alles Unaufgeforderte
als Push auf den Event-Bus.

## Testbarkeit

Ohne Node am USB-Port muss trotzdem alles Wesentliche prüfbar sein:

| Schicht | Wie geprüft |
| --- | --- |
| `meshdash-proto` | Unit-Tests gegen feste Byte-Arrays; Round-Trip-Tests |
| `meshdash-transport` | Mock-Transport, der Frames aus einem Skript liefert |
| `meshdash-core` | SQLite in-memory, synthetische Ereignisse auf den Bus |
| `meshdash-modules` | HTTP-Tests gegen den Router, Mock-Link |
| Frontend | Komponententests gegen gemockte API |

Der Mock-Transport ist keine Testhilfe am Rand, sondern Bestandteil der
Architektur. Details in [`testing.md`](testing.md).

## Persistenz

SQLite über `sqlx`. Jedes Modul bringt seine eigenen Migrationen mit und besitzt
seine eigenen Tabellen; Tabellennamen werden mit dem Modulnamen präfixiert.
Module lesen **nicht** direkt in fremden Tabellen — Querbezüge laufen über den
Event-Bus oder eine vom besitzenden Modul angebotene Schnittstelle.

Kompilierzeit-geprüfte Queries (`sqlx::query!`) setzen eine Datenbank zur Bauzeit
voraus und werden deshalb **nicht** verwendet — sonst braucht jeder Build und
jeder CI-Lauf eine vorbereitete Datenbank. Stattdessen Laufzeit-Queries mit
Tests, die tatsächlich gegen ein Schema laufen.

## Abgrenzung — was MeshDash *nicht* ist

Ohne diese Grenzen wird das Projekt beliebig:

- **Keine Firmware und kein Ersatz dafür.** MeshDash spricht mit einem
  Companion-Node, es ersetzt ihn nicht.
- **Kein zweiter Mesh-Client.** Chat und Kontakte gibt es, weil ein Dashboard
  ohne sie unvollständig wäre — nicht als Konkurrenz zu den offiziellen Apps.
- **Keine Cloud, kein Multi-Tenant.** Eine Instanz gehört einem Betreiber und
  einem Mesh. Kein Mandantenmodell.
- **Kein eigenes Routing.** Wie Pakete durchs Mesh laufen, entscheidet die
  Firmware.

## Offene Punkte

Bewusst noch nicht entschieden — jeweils zu klären, wenn es soweit ist:

- **Authentifizierung.** Einzelnes Token, Benutzer/Passwort oder vorgelagerter
  Reverse-Proxy? Für den Anfang genügt ein optionales Token, aber die
  Entscheidung gehört in einen eigenen ADR.
- **Mehrere Nodes gleichzeitig.** Die Architektur sieht heute einen `Link` vor.
  Mehrere Gateways sind denkbar, aber nicht durchdacht. Nicht ohne ADR anfangen.
- **Aufbewahrungsdauer.** Telemetrie wächst unbegrenzt. Verdichtung oder
  Löschfristen sind ungeklärt.
- **Repeater-Zugangsdaten.** Siehe [`../SECURITY.md`](../SECURITY.md).

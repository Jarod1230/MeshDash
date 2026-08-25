# Architecture Decision Records

Jede Entscheidung, die schwer rückgängig zu machen ist oder die jemand später
plausibel infrage stellen wird, wird hier festgehalten — mit dem **Warum**,
nicht nur dem Was.

Der Zweck ist nicht Bürokratie, sondern der Moment in acht Monaten, in dem
jemand fragt „warum eigentlich SQLite?" und niemand mehr weiß, welche
Alternativen damals auf dem Tisch lagen.

## Regeln

- **ADRs werden nicht umgeschrieben.** Ändert sich eine Entscheidung, entsteht
  ein neuer ADR. Der alte bekommt den Status `Abgelöst durch ADR-XXXX` und
  bleibt stehen.
- Fortlaufend nummeriert, Dateiname `NNNN-titel-mit-bindestrichen.md`.
- Ein ADR pro Entscheidung.
- Die **verworfenen** Alternativen gehören dazu. Ein ADR ohne sie ist eine
  Notiz, keine Entscheidung.

## Status

| Status | Bedeutung |
| --- | --- |
| `Vorschlag` | Zur Diskussion, noch nicht wirksam |
| `Angenommen` | Gilt |
| `Abgelöst durch ADR-XXXX` | Historisch, ersetzt |
| `Verworfen` | Erwogen und abgelehnt — bleibt dokumentiert |

## Bestand

| Nr. | Titel | Status |
| --- | --- | --- |
| [0001](0001-tech-stack.md) | Technologie-Stack: Rust und React | Angenommen |
| [0002](0002-modulare-architektur.md) | Modulare Architektur mit Event-Bus | Angenommen |
| [0003](0003-transport-priorisierung.md) | Serial und TCP zuerst, BLE später | Angenommen |
| [0004](0004-dokumentationssprache.md) | Doku Deutsch, Code Englisch | Angenommen |
| [0005](0005-sqlite-als-datenbank.md) | SQLite als einzige Datenbank | Angenommen |
| [0006](0006-authentifizierung.md) | Einzelnes Token, kein ungeschützter Start nach außen | Angenommen |
| [0007](0007-modul-ereignisse.md) | Module tauschen Daten über ein generisches Ereignis aus | Angenommen |
| [0008](0008-frontend-bausteine.md) | react-router, eigener Datenabruf, eigenes SVG, Systemschriften | Angenommen |
| [0009](0009-cayennelpp.md) | CayenneLPP selbst dekodieren, über `CMD_SEND_BINARY_REQ` | Angenommen |
| [0010](0010-karte.md) | Karte als Ansicht in `nodes`, ohne Kacheln | Abgelöst durch [0011](0011-karte-als-leitansicht.md) |
| [0011](0011-karte-als-leitansicht.md) | Die Karte ist die Leitansicht, Kacheln über MeshDash | Angenommen |

Vorlage: [`template.md`](template.md).

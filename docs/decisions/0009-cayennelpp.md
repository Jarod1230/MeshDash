# ADR-0009: CayenneLPP selbst dekodieren, über den neuen Anfrageweg

- **Status:** Angenommen
- **Datum:** 2026-08-20
- **Betrifft:** `meshdash-proto`, Modul `telemetry`

## Kontext

Telemetrie **fremder** Nodes — Spannung, Temperatur, Luftdruck, Position eines
Repeaters auf dem Berg — steckt in einer CayenneLPP-Nutzlast. Das ist ein
Fremdformat aus der LoRaWAN-Welt; die Firmware bindet dafür
`electroniccats/CayenneLPP @ 1.6.1` ein (`platformio.ini`, Commit `d929643`).

Bisher stand in der Roadmap nur „braucht eine Abhängigkeitsentscheidung". Die
Recherche für diese Entscheidung hat drei Dinge ergeben, die sie beantworten.

**Es gibt keine brauchbare Rust-Bibliothek.** Die einzige ernsthafte Crate,
`cayenne_lpp` (MIT, ohne Abhängigkeiten, `no_std`), **erzeugt** Nutzlasten und
liest sie nicht. MeshDash braucht ausschließlich das Gegenteil. Die Option
„Fremdbibliothek einbinden" existiert also gar nicht.

**Das Format ist am Firmware-Quellcode belegbar.** `LPPDataHelpers.h` enthält
eine vollständige Typtabelle — 28 Typen mit Breite, Vorzeichen und Multiplikator
— und mit `LPPReader` eine Referenzimplementierung des Lesens. Regel 1 ist damit
erfüllbar, ohne sich auf fremde Dokumentation zu stützen.

**Der naheliegende Anfrageweg ist abgekündigt.** `CMD_SEND_TELEMETRY_REQ` (39)
trägt im Quelltext den Vermerk „can deprecate, in favour of
CMD_SEND_BINARY_REQ". Und: Eine Telemetrieantwort kommt **nur** auf eine eigene
Anfrage — die Firmware vergleicht das Tag (`tag == pending_telemetry`). Passiv
mitzuhören und zu sammeln, was ohnehin vorbeikommt, geht nicht.

## Entscheidung

**Der Dekoder wird selbst geschrieben**, in `meshdash-proto`, mit Quellenangabe
je Typ nach `LPPDataHelpers.h`.

**Angefragt wird über `CMD_SEND_BINARY_REQ` (50)**, nicht über den abgekündigten
Weg. Die Anfrage trägt `REQ_TYPE_GET_TELEMETRY_DATA` (`0x03`); die Antwort
kommt als `PUSH_CODE_BINARY_RESPONSE` (`0x8C`).

**In dieser Reihenfolge**, weil jedes Stück für sich prüfbar ist:

1. Dekoder und Anfrage-/Antwortkodierung in `meshdash-proto`, gegen feste
   Byte-Arrays getestet.
2. Das Modul `telemetry` fragt Nachbarn und speichert, was zurückkommt.
3. Die Oberfläche zeigt es.

## Begründung

Selbst schreiben ist hier **weniger** Risiko als eine Abhängigkeit, nicht mehr:
Die Crate kann nicht, was gebraucht wird, und der Umfang ist eine Datei in der
Größenordnung von `contact.rs`. Ein Fremdformat, dessen Referenz im selben
Commit liegt wie alles andere, ist nicht fremder als der Rest des Protokolls.

Auf den abgekündigten Weg zu bauen wäre die schlechtere Wahl, obwohl er heute
funktioniert. Der Dekoder bleibt in beiden Fällen gültig — die Nutzlast ändert
sich nicht —, aber das Kommando müsste zweimal geschrieben werden.

## Verworfene Alternativen

**`cayenne_lpp` einbinden** — kann nur erzeugen. Ein Encoder für einen Empfänger
ist keine halbe Lösung, sondern keine.

**Passiv sammeln, später deuten** — war der ursprüngliche Vorschlag und ist
falsch: Ohne eigene Anfrage sendet der Node nie eine Telemetrieantwort. Geprüft
an `onContactResponse()`.

**`CMD_SEND_TELEMETRY_REQ` (39) nutzen, weil es heute geht** — die Firmware
kündigt es selbst ab.

**Ganz lassen** — bliebe die Frage „wie geht es dem Repeater auf dem Berg"
dauerhaft unbeantwortet. Genau dafür existiert das Projekt.

## Folgen

- Der Dekoder muss **Big-Endian** lesen, obwohl das übrige MeshCore-Protokoll
  durchgehend Little-Endian ist. Eine vertauschte Reihenfolge wirft keinen
  Fehler, sie liefert plausibel aussehenden Unsinn — das ist die Falle dieser
  Änderung und gehört ausdrücklich getestet.
- **Kanal 0 beendet die Daten**, es ist kein Kanal. Wer bis zum Pufferende
  parst, liest über die Nutzlast hinaus.
- Anders als beim alten Weg trägt die Antwort **kein Schlüsselpräfix**, sondern
  nur das Tag der Anfrage. Wer wissen will, von wem die Werte stammen, muss sich
  Tag und Kontakt selbst merken.
- Unbekannte LPP-Typen müssen den Rest der Nutzlast unlesbar machen, weil die
  Breite eines unbekannten Typs unbekannt ist. Der Dekoder gibt zurück, was er
  bis dahin gelesen hat, und markiert den Abbruch — statt zu raten und den
  Rest zu verschieben.

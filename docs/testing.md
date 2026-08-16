# Teststrategie

Die zentrale Randbedingung: **Ein MeshCore-Node ist keine Testvoraussetzung.**
Niemand hat in der CI Hardware am USB-Port, und wer am Projekt mitarbeitet,
soll das auch ohne Funkgerät können. Alles, was sich nur mit Hardware prüfen
lässt, ist faktisch ungetestet.

## Ebenen

### `meshdash-proto` — Byte-Tests

Die wichtigste Ebene. Reine Funktionen von Bytes nach Struktur, ohne I/O,
also mit gewöhnlichen Unit-Tests vollständig abdeckbar.

Pflicht für jede Frame-Art:

- **Round-Trip:** kodieren, dekodieren, Gleichheit prüfen.
- **Feste Byte-Arrays:** ein bekannter Frame aus echtem Verkehr, als Konstante
  im Test, mit Herkunftsangabe im Kommentar. Das ist die einzige Prüfung, die
  eine falsche Annahme über das Wire-Format tatsächlich aufdeckt.
- **Abgeschnittene Frames:** dürfen einen Fehler liefern, aber nicht panicken.
- **Unbekannte Opcodes:** müssen als `Unknown` durchkommen, nicht verworfen werden.
- **Überzählige Bytes:** ein Frame, dem Daten folgen, darf den Decoder nicht
  aus dem Tritt bringen.

Fuzzing des Decoders ist angedacht — der Decoder verarbeitet Fremdeingaben und
ist damit der natürliche Kandidat.

### `meshdash-transport` — Mock-Transport

Der Mock-Transport implementiert dasselbe `Transport`-Trait wie Serial und TCP
und spielt ein Skript ab. Er ist **Bestandteil der Architektur**, kein
Testbehelf: Ohne ihn lässt sich weder der Link noch ein Modul prüfen, und
das Frontend hat keine Datenquelle.

Ein Skript ist eine Folge von Schritten: `Emit` liefert einen Frame, `Drop`
lässt die Verbindung abreißen. Nach einem `Drop` schlagen weitere Zugriffe fehl,
bis erneut verbunden wird — dann läuft das Skript hinter der Abbruchstelle
weiter. So lässt sich ein abgezogenes USB-Kabel nachstellen, ohne eines zu
haben. Läuft das Skript aus, endet die Verbindung ebenfalls mit einem Fehler,
statt auf ewig zu warten; ein Test soll hängen können, aber nicht schweigend.

Skripte werden derzeit im Code zusammengesetzt. Sie aus Dateien zu laden ist
möglich und sinnvoll, sobald es aufgezeichneten Verkehr gibt — siehe
„Fixtures" unten.

Damit prüfbar:

- Antwortkorrelation im `Link`
- Verhalten bei Verbindungsabbruch und Wiederverbindung
- Zeitüberschreitung, wenn der Node nicht antwortet
- Pushes, die zwischen Kommando und Antwort eintreffen

### `meshdash-core` und Module — Integrationstests

- SQLite in-memory pro Test, Migrationen laufen echt durch.
- Synthetische Ereignisse auf den Event-Bus, dann den Datenbankzustand prüfen.
- Modulrouten über den zusammengebauten Router aufrufen, nicht die Handler
  direkt — sonst bleiben Routing und Serialisierung ungeprüft.
- Migrationen mindestens einmal gegen eine bestehende Datenbank testen, nicht
  nur gegen eine leere.

### Frontend

- Komponententests gegen gemockte API-Antworten.
- Ein Ende-zu-Ende-Test über den kompletten Durchstich — Mock-Transport,
  Backend, Browser — ist das Ziel, sobald Schritt 6 der Roadmap steht.

## Fixtures

Aufgezeichneter Verkehr gehört unter `fixtures/`, mit einer Notiz zur Herkunft:
welche Firmware-Version, welche Hardware, wann aufgenommen. Ein Fixture ohne
Herkunftsangabe ist wertlos, weil man später nicht beurteilen kann, ob es noch
gilt.

**Vor dem Ablegen prüfen:** Aufgezeichneter Verkehr kann Nachrichteninhalte,
öffentliche Schlüssel und Positionen enthalten. Was in ein öffentliches
Repository geht, ist zu anonymisieren.

## Was die CI prüfen wird

```
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
pnpm lint && pnpm typecheck && pnpm test && pnpm build
```

Was hier durchfällt, wird nicht gemergt.

## Was bewusst ungetestet bleibt

Ehrlichkeit an dieser Stelle ist besser als Scheinabdeckung:

- **Echte Funkstrecken.** Reichweite, Störungen, Pfadwechsel im Feld lassen
  sich nicht automatisiert prüfen.
- **Verhalten echter Firmware.** Der Mock spielt unsere *Annahme* über die
  Firmware nach. Weicht die Annahme ab, sind die Tests grün und die Software
  falsch — genau deshalb gilt die Regel, Protokollwerte nicht zu raten.
- **Serielle Hardware-Eigenheiten.** Latenzen, Puffergrößen, USB-Reset-Verhalten.
